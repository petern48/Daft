use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use async_trait::async_trait;
use common_daft_config::DaftExecutionConfig;
use common_display::{tree::TreeDisplay, DisplayAs, DisplayLevel};
use common_error::DaftResult;
use common_file_formats::{FileFormatConfig, ParquetSourceConfig};
use common_runtime::{get_compute_pool_num_threads, get_io_runtime};
use common_scan_info::{Pushdowns, ScanTaskLike};
use daft_core::prelude::{AsArrow, Int64Array, SchemaRef, Utf8Array};
use daft_csv::{CsvConvertOptions, CsvParseOptions, CsvReadOptions};
use daft_io::IOStatsRef;
use daft_json::{JsonConvertOptions, JsonParseOptions, JsonReadOptions};
use daft_micropartition::MicroPartition;
use daft_parquet::read::{read_parquet_bulk_async, ParquetSchemaInferenceOptions};
use daft_scan::{ChunkSpec, ScanTask};
use daft_warc::WarcConvertOptions;
use futures::{Stream, StreamExt, TryStreamExt};
use snafu::ResultExt;
use tracing::instrument;

use crate::sources::source::{Source, SourceStream};

pub struct ScanTaskSource {
    scan_tasks: Vec<Arc<ScanTask>>,
    num_parallel_tasks: usize,
    schema: SchemaRef,
}

impl ScanTaskSource {
    const MAX_PARALLEL_SCAN_TASKS: usize = 8;

    pub fn new(
        scan_tasks: Vec<Arc<ScanTask>>,
        pushdowns: Pushdowns,
        schema: SchemaRef,
        cfg: &DaftExecutionConfig,
    ) -> Self {
        // Determine the number of parallel tasks to run based on available CPU cores and row limits
        let mut num_parallel_tasks = match pushdowns.limit {
            // If we have a row limit, we need to calculate how many parallel tasks we can run
            // without exceeding the limit
            Some(limit) => {
                let mut count = 0;
                let mut remaining_rows = limit as f64;

                // Only examine tasks up to the number of available CPU cores
                for scan_task in scan_tasks.iter().take(Self::MAX_PARALLEL_SCAN_TASKS) {
                    match scan_task.approx_num_rows(Some(cfg)) {
                        // If we can estimate the number of rows for this task
                        Some(estimated_rows) => {
                            remaining_rows -= estimated_rows;
                            count += 1;

                            // Stop adding tasks if we would exceed the row limit
                            if remaining_rows <= 0.0 {
                                break;
                            }
                        }
                        // If we can't estimate rows, conservatively include the task
                        // This ensures we don't underutilize available resources
                        None => count += 1,
                    }
                }
                count
            }
            // If there's no row limit, use all available CPU cores
            None => Self::MAX_PARALLEL_SCAN_TASKS,
        };
        num_parallel_tasks = num_parallel_tasks.min(scan_tasks.len());
        Self {
            scan_tasks,
            num_parallel_tasks,
            schema,
        }
    }

    pub fn arced(self) -> Arc<dyn Source> {
        Arc::new(self) as Arc<dyn Source>
    }
}

#[async_trait]
impl Source for ScanTaskSource {
    #[instrument(name = "ScanTaskSource::get_data", level = "info", skip_all)]
    async fn get_data(
        &self,
        maintain_order: bool,
        io_stats: IOStatsRef,
    ) -> DaftResult<SourceStream<'static>> {
        let io_runtime = get_io_runtime(true);
        let delete_map = get_delete_map(&self.scan_tasks).await?.map(Arc::new);
        let stream_of_streams =
            futures::stream::iter(self.scan_tasks.clone().into_iter().map(move |scan_task| {
                let io_stats = io_stats.clone();
                let delete_map = delete_map.clone();
                io_runtime.spawn(stream_scan_task(
                    scan_task,
                    io_stats,
                    delete_map,
                    maintain_order,
                ))
            }));

        match maintain_order {
            true => {
                let buffered_and_flattened = stream_of_streams
                    .buffered(self.num_parallel_tasks)
                    .map(|r| r?)
                    .try_flatten();
                Ok(Box::pin(buffered_and_flattened))
            }
            false => {
                let buffered_and_flattened = stream_of_streams
                    .then(|r| async { r.await? })
                    .try_flatten_unordered(self.num_parallel_tasks);
                Ok(Box::pin(buffered_and_flattened))
            }
        }
    }

    fn name(&self) -> &'static str {
        "ScanTaskSource"
    }

    fn multiline_display(&self) -> Vec<String> {
        self.display_as(DisplayLevel::Default)
            .lines()
            .map(|s| s.to_string())
            .collect()
    }

    fn schema(&self) -> &SchemaRef {
        &self.schema
    }
}

impl TreeDisplay for ScanTaskSource {
    fn display_as(&self, level: DisplayLevel) -> String {
        use std::fmt::Write;
        fn base_display(scan: &ScanTaskSource) -> String {
            let num_scan_tasks = scan.scan_tasks.len();
            let total_bytes: usize = scan
                .scan_tasks
                .iter()
                .map(|st| st.size_bytes_on_disk().unwrap_or(0))
                .sum();

            #[allow(unused_mut)]
            let mut s = format!(
                "ScanTaskSource:
Num Scan Tasks = {num_scan_tasks}
Estimated Scan Bytes = {total_bytes}
"
            );
            #[cfg(feature = "python")]
            if let FileFormatConfig::Database(config) =
                scan.scan_tasks[0].file_format_config().as_ref()
            {
                if num_scan_tasks == 1 {
                    writeln!(s, "SQL Query = {}", &config.sql).unwrap();
                } else {
                    writeln!(s, "SQL Queries = [{},..]", &config.sql).unwrap();
                }
            }
            s
        }
        match level {
            DisplayLevel::Compact => self.get_name(),
            DisplayLevel::Default => {
                let mut s = base_display(self);
                // We're only going to display the pushdowns and schema for the first scan task.
                let pushdown = self.scan_tasks[0].pushdowns();
                if !pushdown.is_empty() {
                    s.push_str(&pushdown.display_as(DisplayLevel::Compact));
                    s.push('\n');
                }

                let schema = self.scan_tasks[0].schema();
                writeln!(
                    s,
                    "Schema: {{{}}}",
                    schema.display_as(DisplayLevel::Compact)
                )
                .unwrap();

                let tasks = self.scan_tasks.iter();

                writeln!(s, "Scan Tasks: [").unwrap();
                for (i, st) in tasks.enumerate() {
                    if i < 3 || i >= self.scan_tasks.len() - 3 {
                        writeln!(s, "{}", st.as_ref().display_as(DisplayLevel::Compact)).unwrap();
                    } else if i == 3 {
                        writeln!(s, "...").unwrap();
                    }
                }
                writeln!(s, "]").unwrap();

                s
            }
            DisplayLevel::Verbose => {
                let mut s = base_display(self);
                writeln!(s, "Scan Tasks: [").unwrap();

                for st in &self.scan_tasks {
                    writeln!(s, "{}", st.as_ref().display_as(DisplayLevel::Verbose)).unwrap();
                }
                s
            }
        }
    }

    fn get_name(&self) -> String {
        "ScanTaskSource".to_string()
    }

    fn get_children(&self) -> Vec<&dyn TreeDisplay> {
        vec![]
    }
}

// Read all iceberg delete files and return a map of file paths to delete positions
async fn get_delete_map(
    scan_tasks: &[Arc<ScanTask>],
) -> DaftResult<Option<HashMap<String, Vec<i64>>>> {
    let delete_files = scan_tasks
        .iter()
        .flat_map(|st| {
            st.sources
                .iter()
                .filter_map(|source| source.get_iceberg_delete_files())
                .flatten()
                .cloned()
        })
        .collect::<HashSet<_>>();
    if delete_files.is_empty() {
        return Ok(None);
    }

    let (runtime, io_client) = scan_tasks
        .first()
        .unwrap() // Safe to unwrap because we checked that the list is not empty
        .storage_config
        .get_io_client_and_runtime()?;
    let scan_tasks = scan_tasks.to_vec();
    runtime
        .spawn(async move {
            let mut delete_map = scan_tasks
                .iter()
                .flat_map(|st| st.sources.iter().map(|s| s.get_path().to_string()))
                .map(|path| (path, vec![]))
                .collect::<std::collections::HashMap<_, _>>();
            let columns_to_read = Some(vec!["file_path".to_string(), "pos".to_string()]);
            let result = read_parquet_bulk_async(
                delete_files.into_iter().collect(),
                columns_to_read,
                None,
                None,
                None,
                None,
                io_client,
                None,
                get_compute_pool_num_threads(),
                ParquetSchemaInferenceOptions::new(None),
                None,
                None,
                None,
                None,
            )
            .await?;

            for table_result in result {
                let table = table_result?;
                // values in the file_path column are guaranteed by the iceberg spec to match the full URI of the corresponding data file
                // https://iceberg.apache.org/spec/#position-delete-files
                let file_paths = table.get_column("file_path")?.downcast::<Utf8Array>()?;
                let positions = table.get_column("pos")?.downcast::<Int64Array>()?;

                for (file, pos) in file_paths
                    .as_arrow()
                    .values_iter()
                    .zip(positions.as_arrow().values_iter())
                {
                    if delete_map.contains_key(file) {
                        delete_map.get_mut(file).unwrap().push(*pos);
                    }
                }
            }
            Ok(Some(delete_map))
        })
        .await?
}

async fn stream_scan_task(
    scan_task: Arc<ScanTask>,
    io_stats: IOStatsRef,
    delete_map: Option<Arc<HashMap<String, Vec<i64>>>>,
    maintain_order: bool,
) -> DaftResult<impl Stream<Item = DaftResult<Arc<MicroPartition>>> + Send> {
    let pushdown_columns = scan_task.pushdowns.columns.as_ref().map(|v| {
        v.iter()
            .map(std::string::String::as_str)
            .collect::<Vec<&str>>()
    });

    let file_column_names = match (
        pushdown_columns,
        scan_task.partition_spec().map(|ps| ps.to_fill_map()),
    ) {
        (None, _) => None,
        (Some(columns), None) => Some(columns.clone()),

        // If the ScanTask has a partition_spec, we elide reads of partition columns from the file
        (Some(columns), Some(partition_fillmap)) => Some(
            columns
                .iter()
                .filter_map(|s| {
                    if partition_fillmap.contains_key(s) {
                        None
                    } else {
                        Some(*s)
                    }
                })
                .collect::<Vec<&str>>(),
        ),
    };

    if scan_task.sources.len() != 1 {
        return Err(common_error::DaftError::TypeError(
            "Streaming reads only supported for single source ScanTasks".to_string(),
        ));
    }
    let source = scan_task.sources.first().unwrap();
    let url = source.get_path();
    let io_config = Arc::new(
        scan_task
            .storage_config
            .io_config
            .clone()
            .unwrap_or_default(),
    );
    let io_client = daft_io::get_io_client(scan_task.storage_config.multithreaded_io, io_config)?;
    let table_stream = match scan_task.file_format_config.as_ref() {
        FileFormatConfig::Parquet(ParquetSourceConfig {
            coerce_int96_timestamp_unit,
            field_id_mapping,
            chunk_size,
            ..
        }) => {
            let inference_options =
                ParquetSchemaInferenceOptions::new(Some(*coerce_int96_timestamp_unit));

            let delete_rows = delete_map.as_ref().and_then(|m| m.get(url).cloned());
            let row_groups = if let Some(ChunkSpec::Parquet(row_groups)) = source.get_chunk_spec() {
                Some(row_groups.clone())
            } else {
                None
            };
            let metadata = scan_task
                .sources
                .first()
                .and_then(|s| s.get_parquet_metadata().cloned());
            daft_parquet::read::stream_parquet(
                url,
                file_column_names.as_deref(),
                scan_task.pushdowns.limit,
                row_groups,
                scan_task.pushdowns.filters.clone(),
                io_client,
                Some(io_stats),
                &inference_options,
                field_id_mapping.clone(),
                metadata,
                maintain_order,
                delete_rows,
                *chunk_size,
            )
            .await?
        }
        FileFormatConfig::Csv(cfg) => {
            let schema_of_file = scan_task.schema.clone();
            let col_names = if !cfg.has_headers {
                Some(schema_of_file.field_names().collect::<Vec<_>>())
            } else {
                None
            };
            let convert_options = CsvConvertOptions::new_internal(
                scan_task.pushdowns.limit,
                file_column_names
                    .as_ref()
                    .map(|cols| cols.iter().map(|col| (*col).to_string()).collect()),
                col_names
                    .as_ref()
                    .map(|cols| cols.iter().map(|col| (*col).to_string()).collect()),
                Some(schema_of_file),
                scan_task.pushdowns.filters.clone(),
            );
            let parse_options = CsvParseOptions::new_with_defaults(
                cfg.has_headers,
                cfg.delimiter,
                cfg.double_quote,
                cfg.quote,
                cfg.allow_variable_columns,
                cfg.escape_char,
                cfg.comment,
            )?;
            let read_options = CsvReadOptions::new_internal(cfg.buffer_size, cfg.chunk_size);
            daft_csv::stream_csv(
                url.to_string(),
                Some(convert_options),
                Some(parse_options),
                Some(read_options),
                io_client,
                Some(io_stats.clone()),
                None,
                // maintain_order, TODO: Implement maintain_order for CSV
            )
            .await?
        }
        FileFormatConfig::Json(cfg) => {
            let schema_of_file = scan_task.schema.clone();
            let convert_options = JsonConvertOptions::new_internal(
                scan_task.pushdowns.limit,
                file_column_names
                    .as_ref()
                    .map(|cols| cols.iter().map(|col| (*col).to_string()).collect()),
                Some(schema_of_file),
                scan_task.pushdowns.filters.clone(),
            );
            let parse_options = JsonParseOptions::new_internal();
            let read_options = JsonReadOptions::new_internal(cfg.buffer_size, cfg.chunk_size);

            daft_json::read::stream_json(
                url.to_string(),
                Some(convert_options),
                Some(parse_options),
                Some(read_options),
                io_client,
                Some(io_stats),
                None,
                // maintain_order, TODO: Implement maintain_order for JSON
            )
            .await?
        }
        FileFormatConfig::Warc(_) => {
            let convert_options = WarcConvertOptions {
                limit: scan_task.pushdowns.limit,
                include_columns: None,
                schema: scan_task.schema.clone(),
                predicate: scan_task.pushdowns.filters.clone(),
            };
            daft_warc::stream_warc(url, io_client, Some(io_stats), convert_options, None).await?
        }
        #[cfg(feature = "python")]
        FileFormatConfig::Database(common_file_formats::DatabaseSourceConfig { sql, conn }) => {
            use pyo3::Python;

            use crate::PyIOSnafu;
            let predicate = scan_task
                .pushdowns
                .filters
                .as_ref()
                .map(|p| (*p.as_ref()).clone().into());
            let table = Python::with_gil(|py| {
                daft_micropartition::python::read_sql_into_py_table(
                    py,
                    sql,
                    conn,
                    predicate.clone(),
                    scan_task.schema.clone().into(),
                    scan_task
                        .pushdowns
                        .columns
                        .as_ref()
                        .map(|cols| cols.as_ref().clone()),
                    scan_task.pushdowns.limit,
                )
                .map(|t| t.into())
                .context(PyIOSnafu)
            })?;
            Box::pin(futures::stream::once(async { Ok(table) }))
        }
        #[cfg(feature = "python")]
        FileFormatConfig::PythonFunction => {
            let iter = daft_micropartition::python::read_pyfunc_into_table_iter(&scan_task)?;
            let stream = futures::stream::iter(iter.map(|r| r.map_err(|e| e.into())));
            Box::pin(stream)
        }
    };

    Ok(table_stream.map(move |table| {
        let table = table?;
        let casted_table = table.cast_to_schema_with_fill(
            scan_task.materialized_schema().as_ref(),
            scan_task
                .partition_spec()
                .as_ref()
                .map(|pspec| pspec.to_fill_map())
                .as_ref(),
        )?;
        let mp = Arc::new(MicroPartition::new_loaded(
            scan_task.materialized_schema(),
            Arc::new(vec![casted_table]),
            scan_task.statistics.clone(),
        ));
        Ok(mp)
    }))
}
