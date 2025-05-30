use std::{
    collections::HashMap,
    fmt::Display,
    hash::{Hash, Hasher},
    ops::{BitAnd, BitOr, Not},
};

use common_error::{DaftError, DaftResult};
use daft_core::prelude::*;
use daft_dsl::{Column, Expr, ExprRef, ResolvedColumn};
use daft_recordbatch::RecordBatch;
use indexmap::{IndexMap, IndexSet};

use crate::column_stats::ColumnRangeStatistics;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TableStatistics {
    pub columns: IndexMap<String, ColumnRangeStatistics>,
}

impl Hash for TableStatistics {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for (key, value) in &self.columns {
            key.hash(state);
            value.hash(state);
        }
    }
}

impl TableStatistics {
    pub fn from_stats_table(table: &RecordBatch) -> DaftResult<Self> {
        // Assumed format is each column having 2 rows:
        // - row 0: Minimum value for the column.
        // - row 1: Maximum value for the column.
        if table.len() != 2 {
            return Err(DaftError::ValueError(format!("Expected stats table to have 2 rows, with min and max values for each column, but got {} rows: {}", table.len(), table)));
        }
        let mut columns = IndexMap::with_capacity(table.num_columns());
        for name in table.column_names() {
            let col = table.get_column(&name).unwrap();
            let stats = ColumnRangeStatistics::new(Some(col.slice(0, 1)?), Some(col.slice(1, 2)?))?;
            columns.insert(name, stats);
        }
        Ok(Self { columns })
    }

    #[must_use]
    pub fn from_table(table: &RecordBatch) -> Self {
        let mut columns = IndexMap::with_capacity(table.num_columns());
        for name in table.column_names() {
            let col = table.get_column(&name).unwrap();
            let stats = ColumnRangeStatistics::from_series(col);
            columns.insert(name, stats);
        }
        Self { columns }
    }

    pub fn union(&self, other: &Self) -> crate::Result<Self> {
        // maybe use the schema from micropartition instead
        let unioned_columns = self
            .columns
            .keys()
            .chain(other.columns.keys())
            .collect::<IndexSet<_>>();
        let mut columns = IndexMap::with_capacity(unioned_columns.len());
        for col in unioned_columns {
            let res_col = match (self.columns.get(col), other.columns.get(col)) {
                (None, None) => panic!("Key missing from both tables; invalid state"),
                (Some(_l), None) => Ok(ColumnRangeStatistics::Missing),
                (None, Some(_r)) => Ok(ColumnRangeStatistics::Missing),
                (Some(l), Some(r)) => l.union(r),
            }?;
            columns.insert(col.clone(), res_col);
        }
        Ok(Self { columns })
    }

    pub fn eval_expression_list(
        &self,
        exprs: &[ExprRef],
        expected_schema: &Schema,
    ) -> crate::Result<Self> {
        let result_cols = exprs
            .iter()
            .map(|e| self.eval_expression(e))
            .collect::<crate::Result<Vec<_>>>()?;

        let new_col_stats = expected_schema
            .field_names()
            .map(ToString::to_string)
            .zip(result_cols)
            .collect();

        Ok(Self {
            columns: new_col_stats,
        })
    }

    pub fn estimate_row_size(&self, schema: Option<&Schema>) -> super::Result<f64> {
        let mut sum_so_far = 0.;

        if let Some(schema) = schema {
            // if schema provided, use it
            for field in schema.fields() {
                let name = field.name.as_str();
                let elem_size = if let Some(stats) = self.columns.get(name) {
                    // first try to use column stats
                    stats.element_size()?
                } else {
                    None
                }
                .or_else(|| {
                    // failover to use dtype estimate
                    field.dtype.estimate_size_bytes()
                })
                .unwrap_or(0.);
                sum_so_far += elem_size;
            }
        } else {
            for elem_size in self
                .columns
                .values()
                .map(super::column_stats::ColumnRangeStatistics::element_size)
            {
                sum_so_far += elem_size?.unwrap_or(0.);
            }
        }

        Ok(sum_so_far)
    }

    pub fn eval_expression(&self, expr: &Expr) -> crate::Result<ColumnRangeStatistics> {
        match expr {
            Expr::Alias(col, _) => self.eval_expression(col.as_ref()),
            Expr::Column(Column::Resolved(ResolvedColumn::Basic(col_name))) => {
                let col = self.columns.get(col_name.as_ref());
                let Some(col) = col else {
                    return Err(crate::Error::DaftCoreCompute {
                        source: DaftError::FieldNotFound(col_name.to_string()),
                    });
                };

                Ok(col.clone())
            }
            Expr::Literal(lit_value) => lit_value.try_into(),
            Expr::Not(col) => self.eval_expression(col)?.not(),
            Expr::BinaryOp { op, left, right } => {
                let lhs = self.eval_expression(left)?;
                let rhs = self.eval_expression(right)?;
                use daft_dsl::Operator::{And, Eq, Gt, GtEq, Lt, LtEq, Minus, NotEq, Or, Plus};
                match op {
                    Lt => lhs.lt(&rhs),
                    LtEq => lhs.lte(&rhs),
                    Eq => lhs.equal(&rhs),
                    NotEq => lhs.not_equal(&rhs),
                    GtEq => lhs.gte(&rhs),
                    Gt => lhs.gt(&rhs),
                    Plus => &lhs + &rhs,
                    Minus => &lhs - &rhs,
                    And => lhs.bitand(&rhs),
                    Or => lhs.bitor(&rhs),
                    _ => Ok(ColumnRangeStatistics::Missing),
                }
            }
            _ => Ok(ColumnRangeStatistics::Missing),
        }
    }

    pub fn cast_to_schema(&self, schema: SchemaRef) -> crate::Result<Self> {
        self.cast_to_schema_with_fill(schema, None)
    }

    pub fn cast_to_schema_with_fill(
        &self,
        schema: SchemaRef,
        fill_map: Option<&HashMap<&str, ExprRef>>,
    ) -> crate::Result<Self> {
        let mut columns = IndexMap::new();
        for field in schema.as_ref() {
            let crs = match self.columns.get(&field.name) {
                Some(column_stat) => column_stat
                    .cast(&field.dtype)
                    .unwrap_or(ColumnRangeStatistics::Missing),
                None => fill_map
                    .as_ref()
                    .and_then(|m| m.get(field.name.as_str()))
                    .map(|e| self.eval_expression(e))
                    .transpose()?
                    .unwrap_or(ColumnRangeStatistics::Missing),
            };
            columns.insert(field.name.clone(), crs);
        }
        Ok(Self { columns })
    }
}

impl Display for TableStatistics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let columns = self
            .columns
            .iter()
            .map(|(s, c)| c.combined_series().unwrap().rename(s))
            .collect::<Vec<_>>();
        let tbl_schema = Schema::new(columns.iter().map(|s| s.field().clone()));
        let tab = RecordBatch::new_with_size(tbl_schema, columns, 2).unwrap();
        write!(f, "{tab}")
    }
}

#[cfg(test)]
mod test {
    use daft_core::prelude::*;
    use daft_dsl::{lit, resolved_col};
    use daft_recordbatch::RecordBatch;

    use super::TableStatistics;
    use crate::column_stats::TruthValue;

    #[test]
    fn test_equal() -> crate::Result<()> {
        let table =
            RecordBatch::from_nonempty_columns(vec![
                Int64Array::from(("a", vec![1, 2, 3, 4])).into_series()
            ])
            .unwrap();
        let table_stats = TableStatistics::from_table(&table);

        // False case
        let expr = resolved_col("a").eq(lit(0));
        let result = table_stats.eval_expression(&expr)?;
        assert_eq!(result.to_truth_value(), TruthValue::False);

        // Maybe case
        let expr = resolved_col("a").eq(lit(3));
        let result = table_stats.eval_expression(&expr)?;
        assert_eq!(result.to_truth_value(), TruthValue::Maybe);

        // True case
        let table =
            RecordBatch::from_nonempty_columns(vec![
                Int64Array::from(("a", vec![0, 0, 0])).into_series()
            ])
            .unwrap();
        let table_stats = TableStatistics::from_table(&table);

        let expr = resolved_col("a").eq(lit(0));
        let result = table_stats.eval_expression(&expr)?;
        assert_eq!(result.to_truth_value(), TruthValue::True);

        Ok(())
    }
}
