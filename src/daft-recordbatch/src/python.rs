use common_error::DaftError;
use daft_core::{
    join::JoinType,
    prelude::*,
    python::{series::PySeries, PySchema},
};
use daft_dsl::python::PyExpr;
use indexmap::IndexMap;
use pyo3::{exceptions::PyValueError, prelude::*};

use crate::{
    ffi,
    file_info::{FileInfo, FileInfos},
    preview::{Preview, PreviewFormat, PreviewOptions},
    RecordBatch,
};

#[pyclass]
#[derive(Clone)]
pub struct PyRecordBatch {
    pub record_batch: RecordBatch,
}

#[pymethods]
impl PyRecordBatch {
    pub fn schema(&self) -> PyResult<PySchema> {
        Ok(PySchema {
            schema: self.record_batch.schema.clone(),
        })
    }

    pub fn cast_to_schema(&self, schema: &PySchema) -> PyResult<Self> {
        Ok(self.record_batch.cast_to_schema(&schema.schema)?.into())
    }

    pub fn eval_expression_list(&self, py: Python, exprs: Vec<PyExpr>) -> PyResult<Self> {
        let converted_exprs: Vec<daft_dsl::ExprRef> = exprs.into_iter().map(|e| e.into()).collect();
        py.allow_threads(|| {
            Ok(self
                .record_batch
                .eval_expression_list(converted_exprs.as_slice())?
                .into())
        })
    }

    pub fn take(&self, py: Python, idx: &PySeries) -> PyResult<Self> {
        py.allow_threads(|| Ok(self.record_batch.take(&idx.series)?.into()))
    }

    pub fn filter(&self, py: Python, exprs: Vec<PyExpr>) -> PyResult<Self> {
        let converted_exprs: Vec<daft_dsl::ExprRef> =
            exprs.into_iter().map(std::convert::Into::into).collect();
        py.allow_threads(|| Ok(self.record_batch.filter(converted_exprs.as_slice())?.into()))
    }

    pub fn sort(
        &self,
        py: Python,
        sort_keys: Vec<PyExpr>,
        descending: Vec<bool>,
        nulls_first: Vec<bool>,
    ) -> PyResult<Self> {
        let converted_exprs: Vec<daft_dsl::ExprRef> = sort_keys
            .into_iter()
            .map(std::convert::Into::into)
            .collect();
        py.allow_threads(|| {
            Ok(self
                .record_batch
                .sort(
                    converted_exprs.as_slice(),
                    descending.as_slice(),
                    nulls_first.as_slice(),
                )?
                .into())
        })
    }

    pub fn argsort(
        &self,
        py: Python,
        sort_keys: Vec<PyExpr>,
        descending: Vec<bool>,
        nulls_first: Vec<bool>,
    ) -> PyResult<PySeries> {
        let converted_exprs: Vec<daft_dsl::ExprRef> = sort_keys
            .into_iter()
            .map(std::convert::Into::into)
            .collect();
        py.allow_threads(|| {
            Ok(self
                .record_batch
                .argsort(
                    converted_exprs.as_slice(),
                    descending.as_slice(),
                    nulls_first.as_slice(),
                )?
                .into())
        })
    }

    pub fn agg(&self, py: Python, to_agg: Vec<PyExpr>, group_by: Vec<PyExpr>) -> PyResult<Self> {
        let converted_to_agg: Vec<daft_dsl::ExprRef> =
            to_agg.into_iter().map(std::convert::Into::into).collect();
        let converted_group_by: Vec<daft_dsl::ExprRef> =
            group_by.into_iter().map(std::convert::Into::into).collect();
        py.allow_threads(|| {
            Ok(self
                .record_batch
                .agg(converted_to_agg.as_slice(), converted_group_by.as_slice())?
                .into())
        })
    }

    pub fn pivot(
        &self,
        py: Python,
        group_by: Vec<PyExpr>,
        pivot_col: PyExpr,
        values_col: PyExpr,
        names: Vec<String>,
    ) -> PyResult<Self> {
        let converted_group_by: Vec<daft_dsl::ExprRef> =
            group_by.into_iter().map(std::convert::Into::into).collect();
        let converted_pivot_col: daft_dsl::ExprRef = pivot_col.into();
        let converted_values_col: daft_dsl::ExprRef = values_col.into();
        py.allow_threads(|| {
            Ok(self
                .record_batch
                .pivot(
                    converted_group_by.as_slice(),
                    converted_pivot_col,
                    converted_values_col,
                    names,
                )?
                .into())
        })
    }

    pub fn hash_join(
        &self,
        py: Python,
        right: &Self,
        left_on: Vec<PyExpr>,
        right_on: Vec<PyExpr>,
        how: JoinType,
    ) -> PyResult<Self> {
        let left_exprs: Vec<daft_dsl::ExprRef> =
            left_on.into_iter().map(std::convert::Into::into).collect();
        let right_exprs: Vec<daft_dsl::ExprRef> =
            right_on.into_iter().map(std::convert::Into::into).collect();
        let null_equals_nulls = vec![false; left_exprs.len()];
        py.allow_threads(|| {
            Ok(self
                .record_batch
                .hash_join(
                    &right.record_batch,
                    left_exprs.as_slice(),
                    right_exprs.as_slice(),
                    null_equals_nulls.as_slice(),
                    how,
                )?
                .into())
        })
    }

    pub fn sort_merge_join(
        &self,
        py: Python,
        right: &Self,
        left_on: Vec<PyExpr>,
        right_on: Vec<PyExpr>,
        is_sorted: bool,
    ) -> PyResult<Self> {
        let left_exprs: Vec<daft_dsl::ExprRef> =
            left_on.into_iter().map(std::convert::Into::into).collect();
        let right_exprs: Vec<daft_dsl::ExprRef> =
            right_on.into_iter().map(std::convert::Into::into).collect();
        py.allow_threads(|| {
            Ok(self
                .record_batch
                .sort_merge_join(
                    &right.record_batch,
                    left_exprs.as_slice(),
                    right_exprs.as_slice(),
                    is_sorted,
                )?
                .into())
        })
    }

    pub fn explode(&self, py: Python, to_explode: Vec<PyExpr>) -> PyResult<Self> {
        let converted_to_explode: Vec<daft_dsl::ExprRef> =
            to_explode.into_iter().map(|e| e.expr).collect();

        py.allow_threads(|| {
            Ok(self
                .record_batch
                .explode(converted_to_explode.as_slice())?
                .into())
        })
    }

    /// Helper to create a rust Preview and use its Display impl.
    #[pyo3(signature = (format, options))]
    pub fn preview(&self, format: &str, options: Option<&str>) -> PyResult<String> {
        let format = PreviewFormat::try_from(format)?;
        let options = match options {
            Some(json) => serde_json::from_str::<PreviewOptions>(json)
                .expect("PreviewOptions produced invalid json."),
            None => PreviewOptions::default(),
        };
        let preview = self.record_batch.clone();
        let preview = Preview::new(preview, format, options);
        Ok(preview.to_string())
    }

    pub fn __repr__(&self) -> PyResult<String> {
        Ok(format!("{}", self.record_batch))
    }

    pub fn _repr_html_(&self) -> PyResult<String> {
        Ok(self.record_batch.repr_html())
    }

    pub fn head(&self, py: Python, num: i64) -> PyResult<Self> {
        if num < 0 {
            return Err(PyValueError::new_err(format!(
                "Can not head Table with negative number: {num}"
            )));
        }
        let num = num as usize;
        py.allow_threads(|| Ok(self.record_batch.head(num)?.into()))
    }

    #[pyo3(signature = (fraction, with_replacement, seed=None))]
    pub fn sample_by_fraction(
        &self,
        py: Python,
        fraction: f64,
        with_replacement: bool,
        seed: Option<u64>,
    ) -> PyResult<Self> {
        if fraction < 0.0 {
            return Err(PyValueError::new_err(format!(
                "Can not sample table with negative fraction: {fraction}"
            )));
        }
        if fraction > 1.0 {
            return Err(PyValueError::new_err(format!(
                "Can not sample table with fraction greater than 1.0: {fraction}"
            )));
        }
        py.allow_threads(|| {
            Ok(self
                .record_batch
                .sample_by_fraction(fraction, with_replacement, seed)?
                .into())
        })
    }

    #[pyo3(signature = (size, with_replacement, seed=None))]
    pub fn sample_by_size(
        &self,
        py: Python,
        size: i64,
        with_replacement: bool,
        seed: Option<u64>,
    ) -> PyResult<Self> {
        if size < 0 {
            return Err(PyValueError::new_err(format!(
                "Can not sample table with negative size: {size}"
            )));
        }
        py.allow_threads(|| {
            Ok(self
                .record_batch
                .sample(size as usize, with_replacement, seed)?
                .into())
        })
    }

    pub fn quantiles(&self, py: Python, num: i64) -> PyResult<Self> {
        if num < 0 {
            return Err(PyValueError::new_err(format!(
                "Can not fetch quantile from table with negative number: {num}"
            )));
        }
        let num = num as usize;
        py.allow_threads(|| Ok(self.record_batch.quantiles(num)?.into()))
    }

    pub fn partition_by_hash(
        &self,
        py: Python,
        exprs: Vec<PyExpr>,
        num_partitions: i64,
    ) -> PyResult<Vec<Self>> {
        if num_partitions < 0 {
            return Err(PyValueError::new_err(format!(
                "Can not partition into negative number of partitions: {num_partitions}"
            )));
        }
        let exprs: Vec<daft_dsl::ExprRef> =
            exprs.into_iter().map(std::convert::Into::into).collect();
        py.allow_threads(|| {
            Ok(self
                .record_batch
                .partition_by_hash(exprs.as_slice(), num_partitions as usize)?
                .into_iter()
                .map(std::convert::Into::into)
                .collect::<Vec<Self>>())
        })
    }

    pub fn partition_by_random(
        &self,
        py: Python,
        num_partitions: i64,
        seed: i64,
    ) -> PyResult<Vec<Self>> {
        if num_partitions < 0 {
            return Err(PyValueError::new_err(format!(
                "Can not partition into negative number of partitions: {num_partitions}"
            )));
        }

        if seed < 0 {
            return Err(PyValueError::new_err(format!(
                "Can not have seed has negative number: {seed}"
            )));
        }
        py.allow_threads(|| {
            Ok(self
                .record_batch
                .partition_by_random(num_partitions as usize, seed as u64)?
                .into_iter()
                .map(std::convert::Into::into)
                .collect::<Vec<Self>>())
        })
    }

    pub fn partition_by_range(
        &self,
        py: Python,
        partition_keys: Vec<PyExpr>,
        boundaries: &Self,
        descending: Vec<bool>,
    ) -> PyResult<Vec<Self>> {
        let exprs: Vec<daft_dsl::ExprRef> = partition_keys
            .into_iter()
            .map(std::convert::Into::into)
            .collect();
        py.allow_threads(|| {
            Ok(self
                .record_batch
                .partition_by_range(
                    exprs.as_slice(),
                    &boundaries.record_batch,
                    descending.as_slice(),
                )?
                .into_iter()
                .map(std::convert::Into::into)
                .collect::<Vec<Self>>())
        })
    }

    pub fn partition_by_value(
        &self,
        py: Python,
        partition_keys: Vec<PyExpr>,
    ) -> PyResult<(Vec<Self>, Self)> {
        let exprs: Vec<daft_dsl::ExprRef> = partition_keys
            .into_iter()
            .map(std::convert::Into::into)
            .collect();
        py.allow_threads(|| {
            let (tables, values) = self.record_batch.partition_by_value(exprs.as_slice())?;
            let pyrecordbatches = tables
                .into_iter()
                .map(std::convert::Into::into)
                .collect::<Vec<Self>>();
            let values = values.into();
            Ok((pyrecordbatches, values))
        })
    }

    pub fn add_monotonically_increasing_id(
        &self,
        py: Python,
        partition_num: u64,
        column_name: &str,
    ) -> PyResult<Self> {
        py.allow_threads(|| {
            Ok(self
                .record_batch
                .add_monotonically_increasing_id(partition_num, 0, column_name)?
                .into())
        })
    }

    pub fn __len__(&self) -> PyResult<usize> {
        Ok(self.record_batch.len())
    }

    pub fn size_bytes(&self) -> PyResult<usize> {
        Ok(self.record_batch.size_bytes()?)
    }

    #[must_use]
    pub fn column_names(&self) -> Vec<String> {
        self.record_batch.column_names()
    }

    pub fn get_column(&self, name: &str) -> PyResult<PySeries> {
        Ok(self.record_batch.get_column(name)?.clone().into())
    }

    pub fn get_column_by_index(&self, idx: i64) -> PyResult<PySeries> {
        if idx < 0 {
            return Err(PyValueError::new_err(format!(
                "Invalid index, negative numbers not supported: {idx}"
            )));
        }
        let idx = idx as usize;
        if idx >= self.record_batch.len() {
            return Err(PyValueError::new_err(format!(
                "Invalid index, out of bounds: {idx} out of {}",
                self.record_batch.len()
            )));
        }

        Ok(self.record_batch.get_column_by_index(idx)?.clone().into())
    }

    #[staticmethod]
    pub fn concat(py: Python, tables: Vec<Self>) -> PyResult<Self> {
        let record_batches: Vec<_> = tables.iter().map(|t| &t.record_batch).collect();
        py.allow_threads(|| Ok(RecordBatch::concat(record_batches.as_slice())?.into()))
    }

    pub fn slice(&self, start: i64, end: i64) -> PyResult<Self> {
        if start < 0 {
            return Err(PyValueError::new_err(format!(
                "slice start can not be negative: {start}"
            )));
        }
        if end < 0 {
            return Err(PyValueError::new_err(format!(
                "slice end can not be negative: {start}"
            )));
        }
        if start > end {
            return Err(PyValueError::new_err(format!(
                "slice length can not be negative: start: {start} end: {end}"
            )));
        }
        Ok(self
            .record_batch
            .slice(start as usize, end as usize)?
            .into())
    }

    #[staticmethod]
    pub fn from_arrow_record_batches(
        py: Python,
        record_batches: Vec<Bound<PyAny>>,
        schema: &PySchema,
    ) -> PyResult<Self> {
        let record_batch =
            ffi::record_batch_from_arrow(py, record_batches.as_slice(), schema.schema.clone())?;
        Ok(Self { record_batch })
    }

    #[staticmethod]
    pub fn from_pylist_series(dict: IndexMap<String, PySeries>) -> PyResult<Self> {
        let mut fields: Vec<Field> = Vec::new();
        let mut columns: Vec<Series> = Vec::new();
        fields.reserve(dict.len());
        columns.reserve(dict.len());

        for (name, series) in dict {
            let series = series.series;
            fields.push(Field::new(name.as_str(), series.data_type().clone()));
            columns.push(series.rename(name));
        }

        let num_rows = columns.first().map_or(0, daft_core::series::Series::len);
        if !columns.is_empty() {
            let first = columns.first().unwrap();
            for s in columns.iter().skip(1) {
                if s.len() != first.len() {
                    return Err(DaftError::ValueError(format!(
                        "Mismatch in Series lengths when making a Table, {} vs {}",
                        s.len(),
                        first.len()
                    ))
                    .into());
                }
            }
        }

        Ok(Self {
            record_batch: RecordBatch::new_with_broadcast(Schema::new(fields), columns, num_rows)?,
        })
    }

    pub fn to_arrow_record_batch(&self) -> PyResult<PyObject> {
        Python::with_gil(|py| {
            let pyarrow = py.import(pyo3::intern!(py, "pyarrow"))?;
            ffi::record_batch_to_arrow(py, &self.record_batch, pyarrow)
        })
    }

    #[staticmethod]
    #[pyo3(signature = (schema=None))]
    pub fn empty(schema: Option<PySchema>) -> PyResult<Self> {
        Ok(RecordBatch::empty(match schema {
            Some(s) => Some(s.schema),
            None => None,
        })?
        .into())
    }

    pub fn to_file_infos(&self) -> PyResult<FileInfos> {
        let file_infos: FileInfos = self.record_batch.clone().try_into()?;
        Ok(file_infos)
    }

    #[staticmethod]
    pub fn from_file_infos(file_infos: &FileInfos) -> PyResult<Self> {
        let table: RecordBatch = file_infos.try_into()?;
        Ok(table.into())
    }
}

impl From<RecordBatch> for PyRecordBatch {
    fn from(value: RecordBatch) -> Self {
        Self {
            record_batch: value,
        }
    }
}

impl From<PyRecordBatch> for RecordBatch {
    fn from(item: PyRecordBatch) -> Self {
        item.record_batch
    }
}

impl AsRef<RecordBatch> for PyRecordBatch {
    fn as_ref(&self) -> &RecordBatch {
        &self.record_batch
    }
}

pub fn register_modules(parent: &Bound<PyModule>) -> PyResult<()> {
    parent.add_class::<PyRecordBatch>()?;
    parent.add_class::<FileInfos>()?;
    parent.add_class::<FileInfo>()?;

    Ok(())
}
