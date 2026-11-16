//! Memory allocated while compiling: bytes, laid out as C lays out each type, so code that
//! looks at memory as another type (`string`'s block, `raw as *StringView`) works as it would
//! when the program runs.
//!
//! Locals hold [`Value`]s; the heap holds [`Byte`]s. A value is encoded as it's written to
//! the heap, and decoded as it's read back, by its type. A pointer in memory is kept as
//! pieces of where it points, so it can be copied byte by byte and still be one.

use crate::{Loc, Value};
use som_mir::*;
use std::rc::Rc;

/// The size of a pointer, and of `usize`: the host's, like the C backend's.
pub(crate) const POINTER: usize = std::mem::size_of::<usize>();

/// A byte of memory allocated while compiling.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Byte {
    /// Never written.
    Uninit,
    Data(u8),
    /// Byte `n` of a pointer to `loc`.
    Ptr(Rc<Loc>, u8),
}

/// Where each part of a value is, as C lays it out.
pub(crate) struct Layout {
    pub size: usize,
    pub align: usize,
    /// Each part's type and offset, in the order `Projection::Field` (or, for an array,
    /// `Projection::Index`) counts them. Empty for a value without parts.
    pub parts: Vec<(TypeId, usize)>,
}

fn align_to(n: usize, align: usize) -> usize {
    n.div_ceil(align) * align
}

/// Lays out `fields` one after another from `start`, as C does: each at the next offset its
/// alignment allows. Their offsets, where they end, and their largest alignment.
fn sequence(mir: &Mir, fields: &[TypeId], start: usize) -> Option<(Vec<usize>, usize, usize)> {
    let (mut offsets, mut end, mut align) = (Vec::new(), start, 1);
    for &field in fields {
        let layout = layout(mir, field)?;
        end = align_to(end, layout.align);
        offsets.push(end);
        end += layout.size;
        align = align.max(layout.align);
    }
    Some((offsets, end, align))
}

/// A type's layout, as the C backend defines it, or `None` for one memory can't hold yet.
pub(crate) fn layout(mir: &Mir, ty: TypeId) -> Option<Layout> {
    let scalar = |size: usize| {
        Some(Layout {
            size,
            align: size,
            parts: Vec::new(),
        })
    };
    let types = &mir.types;
    match types.get(ty) {
        Ty::Unit | Ty::Bool => scalar(1),
        Ty::Char => scalar(4),
        Ty::Int(int) => scalar(match int {
            IntTy::I8 | IntTy::U8 => 1,
            IntTy::I16 | IntTy::U16 => 2,
            IntTy::I32 | IntTy::U32 => 4,
            IntTy::I64 | IntTy::U64 => 8,
            IntTy::I128 | IntTy::U128 => 16,
            IntTy::ISize | IntTy::USize => POINTER,
        }),
        Ty::Float(FloatTy::F32) => scalar(4),
        Ty::Float(FloatTy::F64) => scalar(8),
        // A slice's reference: where its elements start, and how many.
        Ty::Ref { to, .. } if matches!(types.get(*to), Ty::Slice(_) | Ty::Str) => {
            let usize = types.usize;
            Some(Layout {
                size: 2 * POINTER,
                align: POINTER,
                parts: vec![(types.pointer, 0), (usize, POINTER)],
            })
        }
        Ty::Pointer | Ty::Ptr(_) | Ty::CStr | Ty::Ref { .. } | Ty::Fn { .. } => scalar(POINTER),
        Ty::Struct(def, args) if mir.struct_def(*def, args).handle => scalar(POINTER),
        Ty::Struct(def, args) => {
            let s = mir.struct_def(*def, args);
            let types: Vec<TypeId> = s.fields.iter().map(|&(_, ty)| ty).collect();
            let Some(variants) = &s.variants else {
                // C has no empty structs: one with no fields holds a `som_unit`.
                if types.is_empty() {
                    return scalar(1);
                }
                let (offsets, end, align) = sequence(mir, &types, 0)?;
                return Some(Layout {
                    size: align_to(end, align),
                    align,
                    parts: types.into_iter().zip(offsets).collect(),
                });
            };
            // `{ uint32_t tag; <common fields>; union { struct { <own fields> } V; … } as; }`
            let own = |i: usize| s.fields[i].0.contains('.');
            let mut offsets = vec![0; types.len()];
            let common: Vec<usize> = (1..types.len()).filter(|&i| !own(i)).collect();
            let common_types: Vec<TypeId> = common.iter().map(|&i| types[i]).collect();
            let (common_offsets, mut end, mut align) = sequence(mir, &common_types, 4)?;
            align = align.max(4);
            for (&i, offset) in common.iter().zip(common_offsets) {
                offsets[i] = offset;
            }
            // Each variant's own fields, as a struct of their own in the union.
            let mut members = Vec::new();
            let (mut union_size, mut union_align) = (0, 1);
            for v in 0..variants.len() {
                let fields: Vec<usize> = (s.variant_fields(v).iter().copied())
                    .filter(|&i| own(i))
                    .collect();
                if fields.is_empty() {
                    continue;
                }
                let field_types: Vec<TypeId> = fields.iter().map(|&i| types[i]).collect();
                let (field_offsets, member_end, member_align) = sequence(mir, &field_types, 0)?;
                union_size = union_size.max(align_to(member_end, member_align));
                union_align = union_align.max(member_align);
                members.push((fields, field_offsets));
            }
            if !members.is_empty() {
                let union_start = align_to(end, union_align);
                for (fields, field_offsets) in members {
                    for (i, offset) in fields.into_iter().zip(field_offsets) {
                        offsets[i] = union_start + offset;
                    }
                }
                end = union_start + union_size;
                align = align.max(union_align);
            }
            Some(Layout {
                size: align_to(end, align),
                align,
                parts: types.into_iter().zip(offsets).collect(),
            })
        }
        // `{ bool some; T value; }`
        Ty::Optional(inner) => {
            let (offsets, end, align) = sequence(mir, &[types.bool, *inner], 0)?;
            Some(Layout {
                size: align_to(end, align),
                align,
                parts: vec![(types.bool, offsets[0]), (*inner, offsets[1])],
            })
        }
        // `{ T items[N]; }`
        Ty::Array { element, len } => {
            let Ty::Value(som_mir::Value::Int(len)) = types.get(*len) else {
                return None;
            };
            let item = layout(mir, *element)?;
            Some(Layout {
                size: item.size * *len as usize,
                align: item.align,
                parts: (0..*len as usize)
                    .map(|i| (*element, i * item.size))
                    .collect(),
            })
        }
        // `{ T start; T end; }`
        Ty::Range(int) => {
            let size = layout(mir, *int)?.size;
            Some(Layout {
                size: 2 * size,
                align: size,
                parts: vec![(*int, 0), (*int, size)],
            })
        }
        _ => None,
    }
}

/// Why bytes couldn't be read as a value.
pub(crate) enum Unreadable {
    /// It's a type memory can't hold yet.
    Type,
    /// Part of a pointer, read as something else, or bytes read as a pointer.
    Pointer,
}

/// `value`, of type `ty`, as bytes. `None` if it can't be written yet.
pub(crate) fn encode(mir: &Mir, value: &Value, ty: TypeId) -> Option<Vec<Byte>> {
    let layout = layout(mir, ty)?;
    let int = |n: i128| -> Vec<Byte> {
        (n.to_le_bytes()[..layout.size].iter())
            .map(|&b| Byte::Data(b))
            .collect()
    };
    Some(match value {
        Value::Uninit => vec![Byte::Uninit; layout.size],
        Value::Unit => vec![Byte::Data(0)],
        Value::Bool(b) => vec![Byte::Data(*b as u8)],
        Value::Int(n) => int(*n),
        Value::Float(x) => match layout.size {
            4 => (*x as f32).to_le_bytes().map(Byte::Data).to_vec(),
            _ => x.to_le_bytes().map(Byte::Data).to_vec(),
        },
        Value::Null => vec![Byte::Data(0); POINTER],
        Value::Ref(loc) => {
            let loc = Rc::new(loc.clone());
            (0..POINTER as u8)
                .map(|n| Byte::Ptr(loc.clone(), n))
                .collect()
        }
        Value::Slice { start, len } => {
            let mut bytes = match start {
                Some(loc) => encode(mir, &Value::Ref(loc.clone()), mir.types.pointer)?,
                None => vec![Byte::Data(0); POINTER],
            };
            bytes.extend(
                (*len as i128).to_le_bytes()[..POINTER]
                    .iter()
                    .map(|&b| Byte::Data(b)),
            );
            bytes
        }
        Value::Agg(parts) => {
            let mut bytes = vec![Byte::Uninit; layout.size];
            for (i, &(ty, offset)) in layout.parts.iter().enumerate() {
                let part = encode(mir, parts.get(i).unwrap_or(&Value::Uninit), ty)?;
                bytes[offset..offset + part.len()].clone_from_slice(&part);
            }
            bytes
        }
        Value::Func(_) => return None,
    })
}

/// The value of type `ty` that `bytes` hold.
pub(crate) fn decode(mir: &Mir, bytes: &[Byte], ty: TypeId) -> Result<Value, Unreadable> {
    let layout = layout(mir, ty).ok_or(Unreadable::Type)?;
    let bytes = &bytes[..layout.size];
    // The bytes as data: `None` if any was never written.
    let data = || -> Result<Option<Vec<u8>>, Unreadable> {
        let mut data = Vec::new();
        for byte in bytes {
            match byte {
                Byte::Data(b) => data.push(*b),
                Byte::Uninit => return Ok(None),
                Byte::Ptr(..) => return Err(Unreadable::Pointer),
            }
        }
        Ok(Some(data))
    };
    let types = &mir.types;
    Ok(match types.get(ty) {
        Ty::Unit => Value::Unit,
        Ty::Bool => match data()? {
            Some(data) => Value::Bool(data[0] != 0),
            None => Value::Uninit,
        },
        Ty::Int(_) | Ty::Char => match data()? {
            Some(data) => {
                let mut wide = [0u8; 16];
                wide[..data.len()].copy_from_slice(&data);
                let n = i128::from_le_bytes(wide);
                Value::Int(match types.get(ty) {
                    Ty::Int(int) => crate::wrap(n, *int),
                    _ => n,
                })
            }
            None => Value::Uninit,
        },
        Ty::Float(float) => match data()? {
            Some(data) => Value::Float(match float {
                FloatTy::F32 => f32::from_le_bytes(data[..4].try_into().unwrap()) as f64,
                FloatTy::F64 => f64::from_le_bytes(data[..8].try_into().unwrap()),
            }),
            None => Value::Uninit,
        },
        Ty::Ref { to, .. } if matches!(types.get(*to), Ty::Slice(_) | Ty::Str) => {
            let start = match decode(mir, &bytes[..POINTER], types.pointer)? {
                Value::Ref(loc) => Some(loc),
                Value::Null => None,
                _ => return Ok(Value::Uninit),
            };
            match decode(mir, &bytes[POINTER..], types.usize)? {
                Value::Int(len) => Value::Slice {
                    start,
                    len: len as usize,
                },
                _ => Value::Uninit,
            }
        }
        _ if layout.parts.is_empty() => pointer(bytes)?,
        _ => Value::Agg(
            (layout.parts.iter())
                .map(|&(ty, offset)| decode(mir, &bytes[offset..], ty))
                .collect::<Result<_, _>>()?,
        ),
    })
}

/// The pointer `bytes` hold: all of one, or all zeroes for null.
fn pointer(bytes: &[Byte]) -> Result<Value, Unreadable> {
    match &bytes[0] {
        Byte::Uninit => Ok(Value::Uninit),
        Byte::Data(_) if bytes.iter().all(|b| *b == Byte::Data(0)) => Ok(Value::Null),
        Byte::Ptr(loc, 0)
            if (bytes.iter().enumerate()).all(
                |(i, b)| matches!(b, Byte::Ptr(l, n) if Rc::ptr_eq(l, loc) && *n as usize == i),
            ) =>
        {
            Ok(Value::Ref((**loc).clone()))
        }
        _ => Err(Unreadable::Pointer),
    }
}
