from typing import TYPE_CHECKING

from daft.daft import LogicalPlanBuilder as PyLogicalPlanBuilder

if TYPE_CHECKING:
    from daft.catalog.python_catalog import PythonCatalog

class PyIdentifier:
    def __init__(self, namespace: tuple[str, ...], name: str): ...
    @staticmethod
    def from_sql(input: str, normalize: bool): ...
    def eq(self, other: PyIdentifier) -> bool: ...
    def getitem(self, index: int) -> str: ...
    def __len__(self) -> int: ...
    def __repr__(self) -> str: ...

def read_table(name: str) -> PyLogicalPlanBuilder: ...
def register_table(name: str, plan_builder: PyLogicalPlanBuilder) -> str: ...
def register_python_catalog(catalog: PythonCatalog, catalog_name: str | None) -> str: ...
def unregister_catalog(catalog_name: str | None) -> bool: ...
