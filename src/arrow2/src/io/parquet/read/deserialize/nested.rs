use ethnum::I256;

use crate::array::PrimitiveArray;
use crate::error::Error;

use super::*;

/// Converts an iterator of arrays to a trait object returning trait objects
#[inline]
fn remove_nested<'a, I>(iter: I) -> NestedArrayIter<'a>
where
    I: Iterator<Item = Result<(NestedState, Box<dyn Array>)>> + Send + Sync + 'a,
{
    Box::new(iter.map(|x| {
        x.map(|(mut nested, array)| {
            let _ = nested.nested.pop().unwrap(); // the primitive
            (nested, array)
        })
    }))
}

/// Converts an iterator of arrays to a trait object returning trait objects
#[inline]
fn primitive<'a, A, I>(iter: I) -> NestedArrayIter<'a>
where
    A: Array,
    I: Iterator<Item = Result<(NestedState, A)>> + Send + Sync + 'a,
{
    Box::new(iter.map(|x| {
        x.map(|(mut nested, array)| {
            let _ = nested.nested.pop().unwrap(); // the primitive
            (nested, Box::new(array) as _)
        })
    }))
}

#[allow(clippy::too_many_arguments)]
pub fn columns_to_iter_recursive<'a, I>(
    mut columns: Vec<I>,
    mut types: Vec<&PrimitiveType>,
    field: Field,
    mut init: Vec<InitNested>,
    num_rows: usize,
    chunk_size: Option<usize>,
    mut num_values: Vec<usize>,
    is_parent_nullable: bool,
) -> Result<NestedArrayIter<'a>>
where
    I: Pages + 'a,
{
    use crate::datatypes::PhysicalType::*;
    use crate::datatypes::PrimitiveType::*;

    Ok(match field.data_type().to_physical_type() {
        Null => {
            // physical type is i32
            init.push(InitNested::Primitive(field.is_nullable));
            types.pop();
            primitive(null::NestedIter::new(
                columns.pop().unwrap(),
                init,
                field.data_type().clone(),
                num_rows,
                chunk_size,
                num_values.pop().unwrap(),
                is_parent_nullable,
            ))
        }
        Boolean => {
            init.push(InitNested::Primitive(field.is_nullable));
            types.pop();
            primitive(boolean::NestedIter::new(
                columns.pop().unwrap(),
                init,
                num_rows,
                chunk_size,
                num_values.pop().unwrap(),
                is_parent_nullable,
            ))
        }
        Primitive(Int8) => {
            init.push(InitNested::Primitive(field.is_nullable));
            types.pop();
            primitive(primitive::NestedIter::new(
                columns.pop().unwrap(),
                init,
                field.data_type().clone(),
                num_rows,
                chunk_size,
                num_values.pop().unwrap(),
                is_parent_nullable,
                |x: i32| x as i8,
            ))
        }
        Primitive(Int16) => {
            init.push(InitNested::Primitive(field.is_nullable));
            types.pop();
            primitive(primitive::NestedIter::new(
                columns.pop().unwrap(),
                init,
                field.data_type().clone(),
                num_rows,
                chunk_size,
                num_values.pop().unwrap(),
                is_parent_nullable,
                |x: i32| x as i16,
            ))
        }
        Primitive(Int32) => {
            init.push(InitNested::Primitive(field.is_nullable));
            types.pop();
            primitive(primitive::NestedIter::new(
                columns.pop().unwrap(),
                init,
                field.data_type().clone(),
                num_rows,
                chunk_size,
                num_values.pop().unwrap(),
                is_parent_nullable,
                |x: i32| x,
            ))
        }
        Primitive(Int64) => {
            init.push(InitNested::Primitive(field.is_nullable));
            types.pop();
            primitive(primitive::NestedIter::new(
                columns.pop().unwrap(),
                init,
                field.data_type().clone(),
                num_rows,
                chunk_size,
                num_values.pop().unwrap(),
                is_parent_nullable,
                |x: i64| x,
            ))
        }
        Primitive(UInt8) => {
            init.push(InitNested::Primitive(field.is_nullable));
            types.pop();
            primitive(primitive::NestedIter::new(
                columns.pop().unwrap(),
                init,
                field.data_type().clone(),
                num_rows,
                chunk_size,
                num_values.pop().unwrap(),
                is_parent_nullable,
                |x: i32| x as u8,
            ))
        }
        Primitive(UInt16) => {
            init.push(InitNested::Primitive(field.is_nullable));
            types.pop();
            primitive(primitive::NestedIter::new(
                columns.pop().unwrap(),
                init,
                field.data_type().clone(),
                num_rows,
                chunk_size,
                num_values.pop().unwrap(),
                is_parent_nullable,
                |x: i32| x as u16,
            ))
        }
        Primitive(UInt32) => {
            init.push(InitNested::Primitive(field.is_nullable));
            let type_ = types.pop().unwrap();
            match type_.physical_type {
                PhysicalType::Int32 => primitive(primitive::NestedIter::new(
                    columns.pop().unwrap(),
                    init,
                    field.data_type().clone(),
                    num_rows,
                    chunk_size,
                    num_values.pop().unwrap(),
                    is_parent_nullable,
                    |x: i32| x as u32,
                )),
                // some implementations of parquet write arrow's u32 into i64.
                PhysicalType::Int64 => primitive(primitive::NestedIter::new(
                    columns.pop().unwrap(),
                    init,
                    field.data_type().clone(),
                    num_rows,
                    chunk_size,
                    num_values.pop().unwrap(),
                    is_parent_nullable,
                    |x: i64| x as u32,
                )),
                other => {
                    return Err(Error::nyi(format!(
                        "Deserializing UInt32 from {other:?}'s parquet"
                    )))
                }
            }
        }
        Primitive(UInt64) => {
            init.push(InitNested::Primitive(field.is_nullable));
            types.pop();
            primitive(primitive::NestedIter::new(
                columns.pop().unwrap(),
                init,
                field.data_type().clone(),
                num_rows,
                chunk_size,
                num_values.pop().unwrap(),
                is_parent_nullable,
                |x: i64| x as u64,
            ))
        }
        Primitive(Float32) => {
            init.push(InitNested::Primitive(field.is_nullable));
            types.pop();
            primitive(primitive::NestedIter::new(
                columns.pop().unwrap(),
                init,
                field.data_type().clone(),
                num_rows,
                chunk_size,
                num_values.pop().unwrap(),
                is_parent_nullable,
                |x: f32| x,
            ))
        }
        Primitive(Float64) => {
            init.push(InitNested::Primitive(field.is_nullable));
            types.pop();
            primitive(primitive::NestedIter::new(
                columns.pop().unwrap(),
                init,
                field.data_type().clone(),
                num_rows,
                chunk_size,
                num_values.pop().unwrap(),
                is_parent_nullable,
                |x: f64| x,
            ))
        }
        Binary | Utf8 => {
            init.push(InitNested::Primitive(field.is_nullable));
            types.pop();
            remove_nested(binary::NestedIter::<i32, _>::new(
                columns.pop().unwrap(),
                init,
                field.data_type().clone(),
                num_rows,
                chunk_size,
                num_values.pop().unwrap(),
                is_parent_nullable,
            ))
        }
        LargeBinary | LargeUtf8 => {
            init.push(InitNested::Primitive(field.is_nullable));
            types.pop();
            remove_nested(binary::NestedIter::<i64, _>::new(
                columns.pop().unwrap(),
                init,
                field.data_type().clone(),
                num_rows,
                chunk_size,
                num_values.pop().unwrap(),
                is_parent_nullable,
            ))
        }
        _ => match field.data_type().to_logical_type() {
            DataType::Dictionary(key_type, _, _) => {
                // TODO(issue#2537): Daft does not currently support Arrow's dictionary logical
                // type. Furthermore, the reader does not encounter dictionary types since
                // dictionaries of type T are stored as a regular column of type T in parquet, and
                // there are no schema hints for Daft to read such columns as dictionary types.
                // Hence, unlike the other type readers, for now we do not pass on `num_values` as
                // there is currently no way to encounter or test this code. This should be fixed
                // when support for reading the dictionary types is added.
                init.push(InitNested::Primitive(field.is_nullable));
                let type_ = types.pop().unwrap();
                let iter = columns.pop().unwrap();
                let data_type = field.data_type().clone();
                match_integer_type!(key_type, |$K| {
                    dict_read::<$K, _>(iter, init, type_, data_type, num_rows, chunk_size)
                })?
            }
            DataType::List(inner)
            | DataType::LargeList(inner)
            | DataType::FixedSizeList(inner, _) => {
                init.push(InitNested::List(field.is_nullable));
                let iter = columns_to_iter_recursive(
                    columns,
                    types,
                    inner.as_ref().clone(),
                    init,
                    num_rows,
                    chunk_size,
                    num_values,
                    is_parent_nullable || field.is_nullable,
                )?;
                let iter = iter.map(move |x| {
                    let (mut nested, array) = x?;
                    let array = create_list(field.data_type().clone(), &mut nested, array);
                    Ok((nested, array))
                });
                Box::new(iter) as _
            }
            DataType::Decimal(_, _) => {
                init.push(InitNested::Primitive(field.is_nullable));
                let type_ = types.pop().unwrap();
                match type_.physical_type {
                    PhysicalType::Int32 => primitive(primitive::NestedIter::new(
                        columns.pop().unwrap(),
                        init,
                        field.data_type.clone(),
                        num_rows,
                        chunk_size,
                        num_values.pop().unwrap(),
                        is_parent_nullable,
                        |x: i32| x as i128,
                    )),
                    PhysicalType::Int64 => primitive(primitive::NestedIter::new(
                        columns.pop().unwrap(),
                        init,
                        field.data_type.clone(),
                        num_rows,
                        chunk_size,
                        num_values.pop().unwrap(),
                        is_parent_nullable,
                        |x: i64| x as i128,
                    )),
                    PhysicalType::FixedLenByteArray(n) if n > 16 => {
                        return Err(Error::InvalidArgumentError(format!(
                            "Can't decode Decimal128 type from `FixedLenByteArray` of len {n}"
                        )))
                    }
                    PhysicalType::FixedLenByteArray(n) => {
                        let iter = fixed_size_binary::NestedIter::new(
                            columns.pop().unwrap(),
                            init,
                            DataType::FixedSizeBinary(n),
                            num_rows,
                            chunk_size,
                            num_values.pop().unwrap(),
                            is_parent_nullable,
                        );
                        // Convert the fixed length byte array to Decimal.
                        let iter = iter.map(move |x| {
                            let (mut nested, array) = x?;
                            let values = array
                                .values()
                                .chunks_exact(n)
                                .map(|value: &[u8]| super::super::convert_i128(value, n))
                                .collect::<Vec<_>>();
                            let validity = array.validity().cloned();

                            let array: Box<dyn Array> = Box::new(PrimitiveArray::<i128>::try_new(
                                field.data_type.clone(),
                                values.into(),
                                validity,
                            )?);

                            let _ = nested.nested.pop().unwrap(); // the primitive

                            Ok((nested, array))
                        });
                        Box::new(iter)
                    }
                    _ => {
                        return Err(Error::nyi(format!(
                            "Deserializing type for Decimal {:?} from parquet",
                            type_.physical_type
                        )))
                    }
                }
            }
            DataType::Decimal256(_, _) => {
                init.push(InitNested::Primitive(field.is_nullable));
                let type_ = types.pop().unwrap();
                match type_.physical_type {
                    PhysicalType::Int32 => primitive(primitive::NestedIter::new(
                        columns.pop().unwrap(),
                        init,
                        field.data_type.clone(),
                        num_rows,
                        chunk_size,
                        num_values.pop().unwrap(),
                        is_parent_nullable,
                        |x: i32| i256(I256::new(x as i128)),
                    )),
                    PhysicalType::Int64 => primitive(primitive::NestedIter::new(
                        columns.pop().unwrap(),
                        init,
                        field.data_type.clone(),
                        num_rows,
                        chunk_size,
                        num_values.pop().unwrap(),
                        is_parent_nullable,
                        |x: i64| i256(I256::new(x as i128)),
                    )),
                    PhysicalType::FixedLenByteArray(n) if n <= 16 => {
                        let iter = fixed_size_binary::NestedIter::new(
                            columns.pop().unwrap(),
                            init,
                            DataType::FixedSizeBinary(n),
                            num_rows,
                            chunk_size,
                            num_values.pop().unwrap(),
                            is_parent_nullable,
                        );
                        // Convert the fixed length byte array to Decimal.
                        let iter = iter.map(move |x| {
                            let (mut nested, array) = x?;
                            let values = array
                                .values()
                                .chunks_exact(n)
                                .map(|value| i256(I256::new(super::super::convert_i128(value, n))))
                                .collect::<Vec<_>>();
                            let validity = array.validity().cloned();

                            let array: Box<dyn Array> = Box::new(PrimitiveArray::<i256>::try_new(
                                field.data_type.clone(),
                                values.into(),
                                validity,
                            )?);

                            let _ = nested.nested.pop().unwrap(); // the primitive

                            Ok((nested, array))
                        });
                        Box::new(iter) as _
                    }

                    PhysicalType::FixedLenByteArray(n) if n <= 32 => {
                        let iter = fixed_size_binary::NestedIter::new(
                            columns.pop().unwrap(),
                            init,
                            DataType::FixedSizeBinary(n),
                            num_rows,
                            chunk_size,
                            num_values.pop().unwrap(),
                            is_parent_nullable,
                        );
                        // Convert the fixed length byte array to Decimal.
                        let iter = iter.map(move |x| {
                            let (mut nested, array) = x?;
                            let values = array
                                .values()
                                .chunks_exact(n)
                                .map(super::super::convert_i256)
                                .collect::<Vec<_>>();
                            let validity = array.validity().cloned();

                            let array: Box<dyn Array> = Box::new(PrimitiveArray::<i256>::try_new(
                                field.data_type.clone(),
                                values.into(),
                                validity,
                            )?);

                            let _ = nested.nested.pop().unwrap(); // the primitive

                            Ok((nested, array))
                        });
                        Box::new(iter) as _
                    }
                    PhysicalType::FixedLenByteArray(n) => {
                        return Err(Error::InvalidArgumentError(format!(
                            "Can't decode Decimal256 type from from `FixedLenByteArray` of len {n}"
                        )))
                    }
                    _ => {
                        return Err(Error::nyi(format!(
                            "Deserializing type for Decimal {:?} from parquet",
                            type_.physical_type
                        )))
                    }
                }
            }
            DataType::Struct(fields) => {
                let columns = fields
                    .iter()
                    .rev()
                    .map(|f| {
                        let mut init = init.clone();
                        init.push(InitNested::Struct(field.is_nullable));
                        let n = n_columns(&f.data_type);
                        let columns = columns.drain(columns.len() - n..).collect();
                        let types = types.drain(types.len() - n..).collect();
                        let num_values = num_values.drain(num_values.len() - n..).collect();
                        columns_to_iter_recursive(
                            columns,
                            types,
                            f.clone(),
                            init,
                            num_rows,
                            chunk_size,
                            num_values,
                            is_parent_nullable || field.is_nullable,
                        )
                    })
                    .collect::<Result<Vec<_>>>()?;
                let columns = columns.into_iter().rev().collect();
                Box::new(struct_::StructIterator::new(columns, fields.clone()))
            }
            DataType::Map(inner, _) => {
                init.push(InitNested::List(field.is_nullable));
                let iter = columns_to_iter_recursive(
                    columns,
                    types,
                    inner.as_ref().clone(),
                    init,
                    num_rows,
                    chunk_size,
                    num_values,
                    is_parent_nullable || field.is_nullable,
                )?;
                let iter = iter.map(move |x| {
                    let (mut nested, array) = x?;
                    let array = create_map(field.data_type().clone(), &mut nested, array);
                    Ok((nested, array))
                });
                Box::new(iter) as _
            }
            other => {
                return Err(Error::nyi(format!(
                    "Deserializing type {other:?} from parquet"
                )))
            }
        },
    })
}

fn dict_read<'a, K: DictionaryKey, I: 'a + Pages>(
    iter: I,
    init: Vec<InitNested>,
    _type_: &PrimitiveType,
    data_type: DataType,
    num_rows: usize,
    chunk_size: Option<usize>,
) -> Result<NestedArrayIter<'a>> {
    use DataType::*;
    let values_data_type = if let Dictionary(_, v, _) = &data_type {
        v.as_ref()
    } else {
        panic!()
    };

    Ok(match values_data_type.to_logical_type() {
        UInt8 => primitive(primitive::NestedDictIter::<K, _, _, _, _>::new(
            iter,
            init,
            data_type,
            num_rows,
            chunk_size,
            |x: i32| x as u8,
        )),
        UInt16 => primitive(primitive::NestedDictIter::<K, _, _, _, _>::new(
            iter,
            init,
            data_type,
            num_rows,
            chunk_size,
            |x: i32| x as u16,
        )),
        UInt32 => primitive(primitive::NestedDictIter::<K, _, _, _, _>::new(
            iter,
            init,
            data_type,
            num_rows,
            chunk_size,
            |x: i32| x as u32,
        )),
        Int8 => primitive(primitive::NestedDictIter::<K, _, _, _, _>::new(
            iter,
            init,
            data_type,
            num_rows,
            chunk_size,
            |x: i32| x as i8,
        )),
        Int16 => primitive(primitive::NestedDictIter::<K, _, _, _, _>::new(
            iter,
            init,
            data_type,
            num_rows,
            chunk_size,
            |x: i32| x as i16,
        )),
        Int32 | Date32 | Time32(_) | Interval(IntervalUnit::YearMonth) => {
            primitive(primitive::NestedDictIter::<K, _, _, _, _>::new(
                iter,
                init,
                data_type,
                num_rows,
                chunk_size,
                |x: i32| x,
            ))
        }
        Int64 | Date64 | Time64(_) | Duration(_) => {
            primitive(primitive::NestedDictIter::<K, _, _, _, _>::new(
                iter,
                init,
                data_type,
                num_rows,
                chunk_size,
                |x: i64| x as i32,
            ))
        }
        Float32 => primitive(primitive::NestedDictIter::<K, _, _, _, _>::new(
            iter,
            init,
            data_type,
            num_rows,
            chunk_size,
            |x: f32| x,
        )),
        Float64 => primitive(primitive::NestedDictIter::<K, _, _, _, _>::new(
            iter,
            init,
            data_type,
            num_rows,
            chunk_size,
            |x: f64| x,
        )),
        Utf8 | Binary => primitive(binary::NestedDictIter::<K, i32, _>::new(
            iter, init, data_type, num_rows, chunk_size,
        )),
        LargeUtf8 | LargeBinary => primitive(binary::NestedDictIter::<K, i64, _>::new(
            iter, init, data_type, num_rows, chunk_size,
        )),
        FixedSizeBinary(_) => primitive(fixed_size_binary::NestedDictIter::<K, _>::new(
            iter, init, data_type, num_rows, chunk_size,
        )),
        /*

        Timestamp(time_unit, _) => {
            let time_unit = *time_unit;
            return timestamp_dict::<K, _>(
                iter,
                physical_type,
                logical_type,
                data_type,
                chunk_size,
                time_unit,
            );
        }
         */
        other => {
            return Err(Error::nyi(format!(
                "Reading nested dictionaries of type {other:?}"
            )))
        }
    })
}
