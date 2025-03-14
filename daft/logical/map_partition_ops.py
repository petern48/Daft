from __future__ import annotations

from abc import abstractmethod
from typing import TYPE_CHECKING

from daft.expressions import ExpressionsProjection
from daft.logical.schema import Schema

if TYPE_CHECKING:
    from daft.recordbatch import MicroPartition


class MapPartitionOp:
    @abstractmethod
    def get_output_schema(self) -> Schema:
        """Returns the output schema after running this MapPartitionOp."""

    @abstractmethod
    def run(self, input_partition: MicroPartition) -> MicroPartition:
        """Runs this MapPartitionOp on the supplied vPartition."""


class ExplodeOp(MapPartitionOp):
    input_schema: Schema
    explode_columns: ExpressionsProjection

    def __init__(self, input_schema: Schema, explode_columns: ExpressionsProjection) -> None:
        super().__init__()
        self.input_schema = input_schema
        output_fields = []
        explode_columns = ExpressionsProjection([c._explode() for c in explode_columns])
        explode_schema = explode_columns.resolve_schema(input_schema)
        for f in input_schema:
            if f.name in explode_schema.column_names():
                output_fields.append(explode_schema[f.name])
            else:
                output_fields.append(f)

        self.output_schema = Schema._from_field_name_and_types([(f.name, f.dtype) for f in output_fields])
        self.explode_columns = explode_columns

    def get_output_schema(self) -> Schema:
        return self.output_schema

    def run(self, input_partition: MicroPartition) -> MicroPartition:
        return input_partition.explode(self.explode_columns)
