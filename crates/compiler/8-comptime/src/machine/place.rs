use super::Machine;
use crate::foreign;
use crate::memory::{Byte, Unreadable, decode, encode, layout};
use crate::*;
use som_base::Span;

impl Machine<'_, '_> {
    /// Where `place` is, in the innermost frame, and its type.
    pub(crate) fn loc(&self, place: &Place, span: &Span) -> Result<(Loc, TypeId), Stop> {
        let frame = self.frames.len() - 1;
        let mir = self.mir();
        let body = mir.funcs[self.frames[frame].func]
            .body
            .as_ref()
            .expect("a body");
        let mut ty = body.locals[place.local].ty;
        let mut loc = Loc::local(frame, place.local);
        // Just past a slice's `*`: an index counts from where its elements start.
        let mut in_slice = false;
        for &projection in &place.projection {
            let projected = mir.project(ty, projection).expect("checked places");
            match projection {
                Projection::Field(i) => loc = self.part(loc, ty, i, span)?,
                Projection::Index(local) => {
                    let i = count(&self.frames[frame].locals[local.id], span)?;
                    loc = match in_slice {
                        true => self.step(&loc, i, projected, span)?,
                        false => self.part(loc, ty, i, span)?,
                    };
                }
                Projection::Deref => match self.read(&loc, ty, span)? {
                    Value::Ref(target) => loc = target,
                    Value::Slice {
                        start: Some(start), ..
                    } => {
                        loc = start;
                        in_slice = true;
                        ty = projected;
                        continue;
                    }
                    Value::Null | Value::Uninit | Value::Slice { start: None, .. } => {
                        return out_of_bounds(span);
                    }
                    _ => return unsupported("this kind of reference", span),
                },
            }
            ty = projected;
            in_slice = false;
        }
        Ok((loc, ty))
    }

    /// Part `i` of the value of type `ty` at `loc`: a field, or an array's element.
    pub(crate) fn part(
        &self,
        mut loc: Loc,
        ty: TypeId,
        i: usize,
        span: &Span,
    ) -> Result<Loc, Stop> {
        match &mut loc.root {
            Root::Local { .. } => loc.path.push(i),
            Root::Heap { offset, .. } | Root::Foreign { address: offset } => {
                match layout(self.mir(), ty) {
                    Some(layout) if i < layout.parts.len() => *offset += layout.parts[i].1,
                    Some(_) => return out_of_bounds(span),
                    None => return unsupported("memory holding a value of this type", span),
                }
            }
        }
        Ok(loc)
    }

    /// `n` elements of type `element` on from `loc`: in its allocation, or its array.
    pub(crate) fn step(
        &self,
        loc: &Loc,
        n: usize,
        element: TypeId,
        span: &Span,
    ) -> Result<Loc, Stop> {
        let mut loc = loc.clone();
        match (&mut loc.root, loc.path.last_mut()) {
            (Root::Heap { offset, .. } | Root::Foreign { address: offset }, _) => {
                match layout(self.mir(), element) {
                    Some(layout) => *offset = offset.wrapping_add(n * layout.size),
                    None => return unsupported("memory holding a value of this type", span),
                }
            }
            (_, Some(last)) => *last += n,
            (Root::Local { .. }, None) if n == 0 => {}
            (Root::Local { .. }, None) => return out_of_bounds(span),
        }
        Ok(loc)
    }

    pub(crate) fn bytes(
        &self,
        alloc: usize,
        offset: usize,
        size: usize,
        span: &Span,
    ) -> Result<&[Byte], Stop> {
        match self.heap.get(alloc) {
            Some(Some(bytes)) if offset + size <= bytes.len() => Ok(&bytes[offset..offset + size]),
            _ => out_of_bounds(span),
        }
    }

    pub(crate) fn read(&self, loc: &Loc, ty: TypeId, span: &Span) -> Result<Value, Stop> {
        let loc = &self.unpinned(loc);
        match &loc.root {
            &Root::Foreign { address } => {
                let Some(layout) = layout(self.mir(), ty) else {
                    return unsupported("memory holding a value of this type", span);
                };
                let bytes: Vec<Byte> = (foreign::read_real(address, layout.size).into_iter())
                    .map(Byte::Data)
                    .collect();
                decode(self.mir(), &bytes, ty)
                    .or_else(|_| unsupported("memory holding a value of this type", span))
            }
            Root::Local { frame, local } => {
                let mut value = &self.frames[*frame].locals[local.id];
                for &i in &loc.path {
                    value = match (value, i) {
                        (Value::Agg(parts), _) => parts.get(i).unwrap_or(&Value::Uninit),
                        (Value::Uninit, _) => &Value::Uninit,
                        // A slice's reference holds where its elements start, and how many.
                        (Value::Slice { start, .. }, 0) => {
                            return Ok(start.clone().map_or(Value::Null, Value::Ref));
                        }
                        (Value::Slice { len, .. }, 1) => return Ok(Value::Int(*len as i128)),
                        _ => return unsupported("this kind of value's parts", span),
                    };
                }
                Ok(value.clone())
            }
            Root::Heap { alloc, offset } => {
                let Some(layout) = layout(self.mir(), ty) else {
                    return unsupported("memory holding a value of this type", span);
                };
                let bytes = self.bytes(*alloc, *offset, layout.size, span)?;
                decode(self.mir(), bytes, ty).or_else(|e| match e {
                    Unreadable::Type => unsupported("memory holding a value of this type", span),
                    Unreadable::Pointer => unsupported("a pointer's bytes as something else", span),
                })
            }
        }
    }

    pub(crate) fn write(
        &mut self,
        loc: &Loc,
        ty: TypeId,
        value: Value,
        span: &Span,
    ) -> Result<(), Stop> {
        let loc = &self.unpinned(loc);
        match &loc.root {
            &Root::Foreign { address } => {
                let Some(bytes) = encode(self.mir(), &value, ty) else {
                    return unsupported("memory holding a value of this type", span);
                };
                let host = self.host_bytes(&bytes, span)?;
                foreign::write_real(address, &host);
                Ok(())
            }
            Root::Local { frame, local } => {
                let mut slot = &mut self.frames[*frame].locals[local.id];
                for &i in &loc.path {
                    if matches!(slot, Value::Uninit) {
                        *slot = Value::Agg(Vec::new());
                    }
                    slot = match slot {
                        Value::Agg(parts) => {
                            if parts.len() <= i {
                                parts.resize(i + 1, Value::Uninit);
                            }
                            &mut parts[i]
                        }
                        _ => return unsupported("changing part of a slice's reference", span),
                    };
                }
                *slot = value;
                Ok(())
            }
            &Root::Heap { alloc, offset } => {
                let Some(bytes) = encode(self.mir(), &value, ty) else {
                    return unsupported("memory holding a value of this type", span);
                };
                match self.heap.get_mut(alloc) {
                    Some(Some(memory)) if offset + bytes.len() <= memory.len() => {
                        memory[offset..offset + bytes.len()].clone_from_slice(&bytes);
                        Ok(())
                    }
                    _ => out_of_bounds(span),
                }
            }
        }
    }

    pub(crate) fn allocate(&mut self, bytes: Vec<Byte>) -> Loc {
        self.heap.push(Some(bytes));
        Loc::heap(self.heap.len() - 1, 0)
    }

    /// Byte `n` on from `loc`. In a local, that's element `n` of an array of bytes.
    pub(crate) fn byte(&self, loc: &Loc, n: usize, span: &Span) -> Result<Byte, Stop> {
        let loc = &self.unpinned(loc);
        match &loc.root {
            &Root::Foreign { address } => Ok(Byte::Data(
                foreign::read_real(address.wrapping_add(n), 1)[0],
            )),
            &Root::Heap { alloc, offset } => Ok(self.bytes(alloc, offset + n, 1, span)?[0].clone()),
            Root::Local { .. } => {
                let at = self.step(loc, n, self.mir().types.u8, span)?;
                match self.read(&at, self.mir().types.u8, span)? {
                    Value::Int(b) => Ok(Byte::Data(b as u8)),
                    Value::Uninit => Ok(Byte::Uninit),
                    _ => unsupported("this kind of value as bytes", span),
                }
            }
        }
    }

    pub(crate) fn set_byte(
        &mut self,
        loc: &Loc,
        n: usize,
        byte: Byte,
        span: &Span,
    ) -> Result<(), Stop> {
        let loc = &self.unpinned(loc);
        match &loc.root {
            &Root::Foreign { address } => {
                let b = self.host_bytes(&[byte], span)?[0];
                foreign::write_real(address.wrapping_add(n), &[b]);
                Ok(())
            }
            &Root::Heap { alloc, offset } => match self.heap.get_mut(alloc) {
                Some(Some(memory)) if offset + n < memory.len() => {
                    memory[offset + n] = byte;
                    Ok(())
                }
                _ => out_of_bounds(span),
            },
            Root::Local { .. } => {
                let u8 = self.mir().types.u8;
                let at = self.step(loc, n, u8, span)?;
                let value = match byte {
                    Byte::Data(b) => Value::Int(b as i128),
                    Byte::Uninit => Value::Uninit,
                    Byte::Ptr(..) | Byte::Func(..) => {
                        return unsupported("a pointer's bytes in a local", span);
                    }
                };
                self.write(&at, u8, value, span)
            }
        }
    }

    /// The bytes of C text at `loc`, up to its zero byte.
    pub(crate) fn c_text(&self, loc: &Loc, span: &Span) -> Result<Vec<u8>, Stop> {
        let mut text = Vec::new();
        loop {
            match self.byte(loc, text.len(), span)? {
                Byte::Data(0) => return Ok(text),
                Byte::Data(b) => text.push(b),
                _ => return unsupported("text that isn't bytes", span),
            }
        }
    }

    pub(crate) fn data(&self, loc: &Loc, n: usize, span: &Span) -> Result<Vec<u8>, Stop> {
        (0..n)
            .map(|i| match self.byte(loc, i, span)? {
                Byte::Data(b) => Ok(b),
                Byte::Uninit => unsupported("memory before it's written", span),
                Byte::Ptr(..) | Byte::Func(..) => unsupported("a pointer's bytes as data", span),
            })
            .collect()
    }
}
