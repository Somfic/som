//! A function the shader reaches, in GLSL: its signature, then its body.

use som_base::message::code;
use som_base::{Message, Span, msg};
use som_lang::LangStruct;
use som_mir::*;
use std::collections::{HashMap, HashSet};

use crate::Translator;
use crate::error::{NoGpu, Problem};
use crate::names::ident;
use crate::types::Scalar;


/// The name of the variable holding which block runs next.
const NEXT: &str = "bb";

impl Translator<'_> {
    /// Translates `func`, or says why it can't be.
    pub(crate) fn function(&mut self, func: FuncId) {
        let mir = self.mir;
        let function = &mir.funcs[func];
        let Some(body) = &function.body else {
            unreachable!("only functions with bodies are reached")
        };
        let mut b = Body {
            func,
            body,
            names: local_names(body),
            aliases: HashMap::new(),
            snapshots: HashMap::new(),
            span: function.span.clone(),
        };
        match b.translate(self) {
            Ok(texts) => self.functions.push(texts),
            Err(problem) => self.fail(func, problem),
        }
    }
}

/// Each local's GLSL name: a named one's name and number, a temporary's `t` and number, and
/// what's returned `ret`.
fn local_names(body: &som_mir::Body) -> Vec<String> {
    let mut names = vec![String::new(); body.locals.slots()];
    for (local, decl) in body.locals.iter_with_ids() {
        names[local.id] = match (&decl.name, local == RETURN) {
            (_, true) => "ret".into(),
            (Some(name), _) => format!("{}_{}", ident(name), local.id),
            (None, _) => format!("t{}", local.id),
        };
    }
    names
}

struct Body<'a> {
    func: FuncId,
    body: &'a som_mir::Body,
    names: Vec<String>,
    /// Each reference, as the place it refers to written out: a parameter's is its name.
    aliases: HashMap<Local, String>,
    /// The copies of indexes a reference's place reads, made where the reference is: each
    /// copy's name and the local it copies. The place stays the one borrowed even if the
    /// local holding the index changes after.
    snapshots: HashMap<Local, Vec<(String, Local)>>,
    /// The statement being translated, for problems.
    span: Span,
}

impl Body<'_> {
    fn local_ty(&self, local: Local) -> TypeId {
        self.body.locals[local].ty
    }

    fn name(&self, local: Local) -> &str {
        &self.names[local.id]
    }

    fn no_gpu(&self, no: NoGpu, what: Option<Message>) -> Problem {
        no.at(&self.span, what.unwrap_or_else(|| "used here".into()))
    }

    /// The function's prototype and definition.
    fn translate(&mut self, t: &mut Translator) -> Result<(String, String), Problem> {
        let prototype = self.signature(t)?;
        self.find_aliases(t)?;

        // Locals, each declared once at the top: GLSL has no `StorageLive`.
        let mut lines = Vec::new();
        for (local, decl) in self.body.locals.iter_with_ids() {
            let is_param = (1..=self.body.arg_count).contains(&local.id);
            if is_param || self.aliases.contains_key(&local) || t.is_unit(decl.ty) {
                continue;
            }
            self.span = decl.span.clone();
            if t.is_lang(decl.ty, LangStruct::Texture) {
                return Err(Problem {
                    message: "a shader can't keep a texture in a variable".into(),
                    span: decl.span.clone(),
                    label: match &decl.name {
                        Some(name) => msg![code(name), " holds a texture"],
                        None => "this holds a texture".into(),
                    },
                    hint: Some(msg![
                        "use it where it's named, ",
                        code("u.image.sample(uv)"),
                        ", or pass a ",
                        code("&Texture")
                    ]),
                });
            }
            let what = match &decl.name {
                Some(name) => msg![code(name), " is a ", t.ty_part(decl.ty)],
                None => msg!["this is a ", t.ty_part(decl.ty)],
            };
            let ty = t.ty(decl.ty).map_err(|no| self.no_gpu(no, Some(what)))?;
            lines.push(format!("    {ty} {};", self.name(local)));
        }
        for snapshots in self.snapshots.values() {
            for (name, _) in snapshots {
                lines.push(format!("    uint {name};"));
            }
        }

        let ret = self.ret(t)?;
        let body = self.body;
        // A block only a `goto` from one other block reaches runs as the end of that one.
        let mut reached_from: HashMap<BlockId, usize> = HashMap::new();
        for block in body.blocks.iter() {
            for to in block.terminator.kind.successors() {
                *reached_from.entry(to).or_default() += 1;
            }
        }
        let entry = BlockId::new(0);
        let joined: HashSet<BlockId> = (body.blocks.iter_with_ids())
            .filter_map(|(id, block)| match block.terminator.kind {
                TerminatorKind::Goto(to) if to != entry && to != id && reached_from[&to] == 1 => {
                    Some(to)
                }
                _ => None,
            })
            .collect();
        // Blocks nothing reaches, like the end of a function whose every branch returns, are
        // left out.
        let mut reachable = HashSet::from([entry]);
        let mut stack = vec![entry];
        while let Some(id) = stack.pop() {
            for to in body.blocks[id].terminator.kind.successors() {
                if reachable.insert(to) {
                    stack.push(to);
                }
            }
        }
        let heads: Vec<BlockId> = (body.blocks.iter_with_ids())
            .map(|(id, _)| id)
            .filter(|id| reachable.contains(id) && !joined.contains(id))
            .collect();
        let chain_end = |mut id: BlockId| loop {
            match body.blocks[id].terminator.kind {
                TerminatorKind::Goto(to) if joined.contains(&to) => id = to,
                ref kind => return kind,
            }
        };
        let straight = heads == [entry]
            && matches!(
                chain_end(entry),
                TerminatorKind::Return | TerminatorKind::Unreachable
            );
        if straight {
            self.chain(t, entry, &joined, &ret, &mut lines, "    ")?;
        } else if let Some(nodes) = crate::structure::structure(body) {
            // `if`s and loops, as the program was written.
            self.nodes(t, &nodes, &ret, &mut lines, 1)?;
            // A function ending in a loop that only returns: GLSL wants every path to.
            if let Some(zero) = &ret
                && matches!(nodes.last(), Some(crate::structure::Node::Loop(_)))
            {
                lines.push(format!("    return {zero};"));
            }
        } else {
            // Any shape of control flow: a `switch` on the block to run, in a loop. GLSL has
            // no `goto`.
            lines.push(format!("    uint {NEXT} = 0u;"));
            lines.push("    for (;;) {".into());
            lines.push(format!("        switch ({NEXT}) {{"));
            for head in heads {
                lines.push(format!("        case {}u: {{", head.id));
                self.chain(t, head, &joined, &ret, &mut lines, "            ")?;
                lines.push("        }".into());
            }
            lines.push("        }".into());
            lines.push("    }".into());
            // Never reached; GLSL wants every path to return.
            if let Some(zero) = &ret {
                lines.push(format!("    return {zero};"));
            }
        }
        let definition = format!("{} {{\n{}\n}}\n", prototype, lines.join("\n"));
        Ok((prototype, definition))
    }

    /// Structured control flow, `depth` levels in.
    fn nodes(
        &mut self,
        t: &mut Translator,
        nodes: &[crate::structure::Node],
        ret: &Option<String>,
        lines: &mut Vec<String>,
        depth: usize,
    ) -> Result<(), Problem> {
        use crate::structure::Node;
        let indent = "    ".repeat(depth);
        for node in nodes {
            match node {
                Node::Block(id) => {
                    for statement in &self.body.blocks[*id].stmts {
                        self.statement(t, statement, lines, &indent)?;
                    }
                }
                Node::If {
                    on,
                    then,
                    otherwise,
                } => {
                    let block = &self.body.blocks[*on];
                    self.span = block.terminator.span.clone();
                    let TerminatorKind::Branch { condition, .. } = &block.terminator.kind else {
                        unreachable!("an `if` is on a branch")
                    };
                    let condition = self.operand(t, condition)?;
                    // Only an `else`: the condition turned around, `if (!c)`.
                    let (mut then_lines, mut otherwise_lines) = (Vec::new(), Vec::new());
                    self.nodes(t, then, ret, &mut then_lines, depth + 1)?;
                    self.nodes(t, otherwise, ret, &mut otherwise_lines, depth + 1)?;
                    match (then_lines.is_empty(), otherwise_lines.is_empty()) {
                        (true, true) => {}
                        (true, false) => {
                            lines.push(format!("{indent}if (!{condition}) {{"));
                            lines.append(&mut otherwise_lines);
                            lines.push(format!("{indent}}}"));
                        }
                        (false, empty) => {
                            lines.push(format!("{indent}if ({condition}) {{"));
                            lines.append(&mut then_lines);
                            if !empty {
                                lines.push(format!("{indent}}} else {{"));
                                lines.append(&mut otherwise_lines);
                            }
                            lines.push(format!("{indent}}}"));
                        }
                    }
                }
                Node::Loop(body) => {
                    lines.push(format!("{indent}for (;;) {{"));
                    self.nodes(t, body, ret, lines, depth + 1)?;
                    lines.push(format!("{indent}}}"));
                }
                Node::Break => lines.push(format!("{indent}break;")),
                Node::Continue => lines.push(format!("{indent}continue;")),
                Node::Return(id) => {
                    let block = &self.body.blocks[*id];
                    self.span = block.terminator.span.clone();
                    lines.push(format!("{indent}{}", self.end(t, &block.terminator.kind, ret)?));
                }
            }
        }
        Ok(())
    }

    /// Block `id`, then each block joined to it in turn, then where the last one goes.
    fn chain(
        &mut self,
        t: &mut Translator,
        mut id: BlockId,
        joined: &HashSet<BlockId>,
        ret: &Option<String>,
        lines: &mut Vec<String>,
        indent: &str,
    ) -> Result<(), Problem> {
        loop {
            let block = &self.body.blocks[id];
            for statement in &block.stmts {
                self.statement(t, statement, lines, indent)?;
            }
            match block.terminator.kind {
                TerminatorKind::Goto(to) if joined.contains(&to) => id = to,
                ref kind => {
                    self.span = block.terminator.span.clone();
                    lines.push(format!("{indent}{}", self.end(t, kind, ret)?));
                    return Ok(());
                }
            }
        }
    }

    /// What `return` gives if the function returns a value: a zero of its type, for paths
    /// that never get there.
    fn ret(&self, t: &mut Translator) -> Result<Option<String>, Problem> {
        let ty = t.mir.funcs[self.func].ret;
        if t.is_unit(ty) {
            return Ok(None);
        }
        t.zero(ty)
            .map(Some)
            .map_err(|no| self.no_gpu(no, Some("returned here".into())))
    }

    fn end(
        &mut self,
        t: &mut Translator,
        kind: &TerminatorKind,
        ret: &Option<String>,
    ) -> Result<String, Problem> {
        Ok(match kind {
            TerminatorKind::Goto(to) => format!("{NEXT} = {}u; break;", to.id),
            TerminatorKind::Branch {
                condition,
                then,
                otherwise,
            } => {
                let condition = self.operand(t, condition)?;
                format!(
                    "{NEXT} = {condition} ? {}u : {}u; break;",
                    then.id, otherwise.id
                )
            }
            TerminatorKind::Return => match ret {
                Some(_) => "return ret;".into(),
                None => "return;".into(),
            },
            // Never reached.
            TerminatorKind::Unreachable => match ret {
                Some(zero) => format!("return {zero};"),
                None => "return;".into(),
            },
        })
    }

    fn signature(&mut self, t: &mut Translator) -> Result<String, Problem> {
        let function = &t.mir.funcs[self.func];
        self.span = crate::head(&function.span);
        let ret = (t.ty(function.ret)).map_err(|no| {
            self.no_gpu(no, Some(msg!["it returns a ", t.ty_part(function.ret)]))
        })?;
        let mut params = Vec::new();
        for (i, &param) in function.params.iter().enumerate() {
            let local = Local::new(i + 1);
            let decl = &self.body.locals[local];
            if t.is_unit(param) {
                continue;
            }
            self.span = decl.span.clone();
            let (prefix, ty) = match t.mir.types.get(param) {
                Ty::Ref { mutable, to }
                    if !matches!(t.mir.types.get(*to), Ty::Slice(_) | Ty::Str) =>
                {
                    self.aliases.insert(local, self.names[local.id].clone());
                    (if *mutable { "inout " } else { "" }, *to)
                }
                _ => ("", param),
            };
            let what = match &decl.name {
                Some(name) => msg![code(name), " is a ", t.ty_part(param)],
                None => msg!["it takes a ", t.ty_part(param)],
            };
            let glsl = t.ty(ty).map_err(|no| self.no_gpu(no, Some(what)))?;
            params.push(format!("{prefix}{glsl} {}", self.name(local)));
        }
        let name = t.func_name(self.func);
        Ok(format!("{ret} {name}({})", params.join(", ")))
    }

    /// Each local reference's place: references aren't values on the GPU, so each use of one
    /// is the place it refers to.
    fn find_aliases(&mut self, t: &mut Translator) -> Result<(), Problem> {
        let body = self.body;
        let is_ref = |t: &Translator, local: Local| {
            matches!(t.mir.types.get(body.locals[local].ty), Ty::Ref { .. })
        };
        // A reference is made before it's copied, but not always in an earlier block.
        loop {
            let mut progress = false;
            for block in body.blocks.iter() {
                for statement in &block.stmts {
                    let StatementKind::Assign(place, rvalue) = &statement.kind else {
                        continue;
                    };
                    let local = place.local;
                    if !place.projection.is_empty()
                        || !is_ref(t, local)
                        || (1..=body.arg_count).contains(&local.id)
                    {
                        continue;
                    }
                    self.span = statement.span.clone();
                    let text = match rvalue {
                        Rvalue::Ref { place, .. } => {
                            let base = place.local;
                            if is_ref(t, base) && !self.aliases.contains_key(&base) {
                                continue;
                            }
                            self.snapshots.remove(&local);
                            self.place(t, place, Some(local))?
                        }
                        Rvalue::Use(Operand::Copy(from) | Operand::Move(from))
                            if from.projection.is_empty() =>
                        {
                            match self.aliases.get(&from.local) {
                                Some(text) => text.clone(),
                                None => continue,
                            }
                        }
                        Rvalue::Call { func, .. } => {
                            let name = crate::short_fn(&t.mir.funcs[*func].name);
                            return Err(Problem {
                                message: msg![
                                    "a shader can't use the reference ",
                                    code(&name),
                                    " returns"
                                ],
                                span: statement.span.clone(),
                                label: msg![code(format!("{name}(…)")), " is called here"],
                                hint: Some(
                                    "on the GPU, references only go into calls: return a copy"
                                        .into(),
                                ),
                            });
                        }
                        _ => return Err(self.kept_reference()),
                    };
                    match self.aliases.get(&local) {
                        Some(existing) if *existing != text => {
                            return Err(Problem {
                                message: "a shader can't have a reference that refers to \
                                          different places"
                                    .into(),
                                span: statement.span.clone(),
                                label: "it refers to another place here".into(),
                                hint: Some("copy the value instead".into()),
                            });
                        }
                        Some(_) => {}
                        None => {
                            self.aliases.insert(local, text);
                            progress = true;
                        }
                    }
                }
            }
            if !progress {
                break;
            }
        }
        Ok(())
    }

    fn kept_reference(&self) -> Problem {
        Problem {
            message: "a shader can't keep a reference".into(),
            span: self.span.clone(),
            label: "kept here".into(),
            hint: Some(msg![
                "on the GPU, a ",
                code("&"),
                " can only be passed to a function"
            ]),
        }
    }

    /// `place` written out. With `snapshot`, it's the place reference `snapshot` refers to,
    /// so the indexes it reads are copied where it's made.
    fn place(
        &mut self,
        t: &mut Translator,
        place: &Place,
        snapshot: Option<Local>,
    ) -> Result<String, Problem> {
        let mut ty = self.local_ty(place.local);
        let mut projections = place.projection.iter();
        let mut text = match self.aliases.get(&place.local) {
            Some(alias) => {
                match projections.next() {
                    Some(Projection::Deref) => {}
                    // The reference itself, as a value.
                    _ => return Err(self.kept_reference()),
                }
                let Ty::Ref { to, .. } = t.mir.types.get(ty) else {
                    unreachable!("only references are aliases")
                };
                ty = *to;
                alias.clone()
            }
            None => self.name(place.local).to_string(),
        };
        for projection in projections {
            match *projection {
                Projection::Deref => return Err(self.kept_reference()),
                // A `Vector`'s array is the vector itself.
                Projection::Field(0) if t.vector(ty).is_some() => {
                    ty = t.mir.project(ty, Projection::Field(0)).expect("validated MIR");
                }
                // A texture among the uniforms is a GLSL uniform of its own: by its name.
                Projection::Field(index)
                    if Some(ty) == t.uniforms
                        && (t.mir.project(ty, Projection::Field(index)))
                            .is_some_and(|f| t.is_lang(f, LangStruct::Texture)) =>
                {
                    let Ty::Struct(def, args) = t.mir.types.get(ty) else {
                        unreachable!("uniforms are a struct")
                    };
                    text = t.mir.struct_def(*def, args).fields[index].0.clone();
                    ty = t.mir.project(ty, Projection::Field(index)).expect("validated MIR");
                }
                Projection::Field(index) => {
                    let member = match t.mir.types.get(ty) {
                        Ty::Ref { .. } => return Err(self.kept_reference()),
                        _ => (t.member_name(ty, index)).map_err(|no| self.no_gpu(no, None))?,
                    };
                    ty = t.mir.project(ty, Projection::Field(index)).expect("validated MIR");
                    text = format!("{text}.{member}");
                }
                Projection::Index(index) => {
                    let index_text = match snapshot {
                        Some(reference) => {
                            let copies = self.snapshots.entry(reference).or_default();
                            let name = format!("{}_i{}", self.names[reference.id], copies.len());
                            copies.push((name.clone(), index));
                            name
                        }
                        None => self.name(index).to_string(),
                    };
                    ty = t.mir.project(ty, Projection::Index(index)).expect("validated MIR");
                    text = format!("{text}[{index_text}]");
                }
            }
        }
        Ok(text)
    }

    fn operand(&mut self, t: &mut Translator, operand: &Operand) -> Result<String, Problem> {
        match operand {
            Operand::Const(c) => t.constant(c).map_err(|no| self.no_gpu(no, None)),
            Operand::Copy(place) | Operand::Move(place) => {
                if place.projection.is_empty()
                    && let Some(alias) = self.aliases.get(&place.local)
                {
                    return Ok(alias.clone());
                }
                self.place(t, place, None)
            }
        }
    }

    fn statement(
        &mut self,
        t: &mut Translator,
        statement: &Statement,
        lines: &mut Vec<String>,
        indent: &str,
    ) -> Result<(), Problem> {
        self.span = statement.span.clone();
        match &statement.kind {
            StatementKind::StorageLive(_) | StatementKind::StorageDead(_) => Ok(()),
            StatementKind::Drop { place, .. } => {
                let ty = t.mir.place_ty(&self.body.locals, place);
                Err(Problem {
                    message: msg!["the GPU can't clean up a ", t.ty_part(ty)],
                    span: statement.span.clone(),
                    label: "dropped here".into(),
                    hint: Some("a shader's values have to be ones that copy".into()),
                })
            }
            StatementKind::Assign(place, rvalue) => {
                if place.projection.is_empty() && self.aliases.contains_key(&place.local) {
                    // A reference made: only the indexes it reads are copied.
                    for (name, index) in self.snapshots.get(&place.local).into_iter().flatten() {
                        lines.push(format!("{indent}{name} = {};", self.name(*index)));
                    }
                    return Ok(());
                }
                let ty = t.mir.place_ty(&self.body.locals, place);
                if matches!(t.mir.types.get(ty), Ty::Ref { .. }) {
                    return Err(self.kept_reference());
                }
                let value = self.rvalue(t, rvalue, ty)?;
                if t.is_unit(ty) {
                    if rvalue.is_call() {
                        lines.push(format!("{indent}{value};"));
                    }
                    return Ok(());
                }
                let to = self.place(t, place, None)?;
                lines.push(format!("{indent}{to} = {value};"));
                Ok(())
            }
        }
    }

    fn rvalue(&mut self, t: &mut Translator, rvalue: &Rvalue, ty: TypeId) -> Result<String, Problem> {
        Ok(match rvalue {
            Rvalue::Use(op) => self.operand(t, op)?,
            Rvalue::Ref { .. } => return Err(self.kept_reference()),
            Rvalue::Binary { op, lhs, rhs } => {
                let operand_ty = t.mir.operand_ty(&self.body.locals, lhs);
                let scalar = t.scalar(operand_ty).map_err(|no| self.no_gpu(no, None))?;
                let (a, b) = (self.operand(t, lhs)?, self.operand(t, rhs)?);
                binary(*op, scalar, &a, &b)
            }
            Rvalue::Not(op) => {
                let operand_ty = t.mir.operand_ty(&self.body.locals, op);
                let text = self.operand(t, op)?;
                match t.mir.types.get(operand_ty) {
                    Ty::Bool => format!("!{text}"),
                    _ => format!("~{text}"),
                }
            }
            Rvalue::Some(op) => {
                let name = t.ty(ty).map_err(|no| self.no_gpu(no, None))?;
                format!("{name}(true, {})", self.operand(t, op)?)
            }
            Rvalue::Array { items, .. } => {
                let name = t.ty(ty).map_err(|no| self.no_gpu(no, None))?;
                let items = (items.iter())
                    .map(|op| self.operand(t, op))
                    .collect::<Result<Vec<_>, _>>()?;
                format!("{name}({})", items.join(", "))
            }
            Rvalue::Repeat { operand, .. } => {
                let name = t.ty(ty).map_err(|no| self.no_gpu(no, None))?;
                let Ty::Array { len, .. } = t.mir.types.get(ty) else {
                    unreachable!("a repeat makes an array")
                };
                let n = t.mir.types.len(*len) as usize;
                let item = self.operand(t, operand)?;
                format!("{name}({})", vec![item; n].join(", "))
            }
            Rvalue::Range { start, end, .. } => {
                let name = t.ty(ty).map_err(|no| self.no_gpu(no, None))?;
                format!("{name}({}, {})", self.operand(t, start)?, self.operand(t, end)?)
            }
            Rvalue::MakeSlice { .. } => {
                return Err(NoGpu::new("slices", None).at(&self.span, "made here".into()));
            }
            Rvalue::Cast { operand, ty: to } => {
                let from = t.mir.operand_ty(&self.body.locals, operand);
                let text = self.operand(t, operand)?;
                let (from, to) = (
                    t.scalar(from).map_err(|no| self.no_gpu(no, None))?,
                    t.scalar(*to).map_err(|no| self.no_gpu(no, None))?,
                );
                match (from, to) {
                    (Some(from), Some(to)) if from == to => text,
                    (Some(_), Some(to)) => format!("{}({text})", to.name()),
                    _ => return Err(self.kept_reference()),
                }
            }
            Rvalue::Struct {
                ty: struct_ty,
                variant,
                fields,
            } => self.construct(t, *struct_ty, *variant, fields)?,
            Rvalue::Call { func, args } => self.call(t, *func, args)?,
            Rvalue::CallIndirect { .. } => {
                return Err(Problem {
                    message: "a shader can't call a function through its address".into(),
                    span: self.span.clone(),
                    label: "called here".into(),
                    hint: Some("call the function by its name".into()),
                });
            }
            Rvalue::Intrinsic { .. } => {
                return Err(Problem {
                    message: "a shader can't use raw memory".into(),
                    span: self.span.clone(),
                    label: "used here".into(),
                    hint: Some("the GPU has no memory to allocate; use arrays".into()),
                });
            }
        })
    }

    fn construct(
        &mut self,
        t: &mut Translator,
        ty: TypeId,
        variant: Option<usize>,
        fields: &[Operand],
    ) -> Result<String, Problem> {
        // A `Vector` is its array.
        if t.vector(ty).is_some() {
            return self.operand(t, &fields[0]);
        }
        let name = t.ty(ty).map_err(|no| self.no_gpu(no, None))?;
        let members = t.members(ty).map_err(|no| self.no_gpu(no, None))?;
        if members.is_empty() {
            return Ok(format!("{name}(false)"));
        }
        // An enum's variant sets its tag and its own fields; the rest are zero.
        let given: HashMap<usize, &Operand> = match variant {
            Some(variant) => {
                let Ty::Struct(def, args) = t.mir.types.get(ty) else {
                    unreachable!("variants are of enums")
                };
                let s = t.mir.struct_def(*def, args);
                s.variant_fields(variant).iter().copied().zip(fields).collect()
            }
            None => fields.iter().enumerate().collect(),
        };
        let mut parts = Vec::new();
        for (_, member_ty, index) in members {
            parts.push(match (variant, given.get(&index)) {
                (_, Some(op)) => self.operand(t, op)?,
                (Some(variant), None) if index == 0 => format!("{variant}u"),
                (_, None) => t.zero(member_ty).map_err(|no| self.no_gpu(no, None))?,
            });
        }
        Ok(format!("{name}({})", parts.join(", ")))
    }

    fn call(&mut self, t: &mut Translator, func: FuncId, args: &[Operand]) -> Result<String, Problem> {
        let callee = &t.mir.funcs[func];
        let name = crate::short_fn(&callee.name);
        if t.mir.fail == Some(func) {
            return Err(Problem {
                message: "a shader can't fail".into(),
                span: self.span.clone(),
                label: "it can fail here".into(),
                hint: Some("the GPU can't stop the program, or say why".into()),
            });
        }
        // One of GLSL's own, from `som::gl`; any other extern function is C's.
        let builtin = match (callee.body.is_none(), t.mir.gpu.contains(&func)) {
            (false, _) => None,
            (true, true) => Some(callee.link_name.clone().unwrap_or_else(|| name.clone())),
            (true, false) => {
                let c_name = callee.link_name.as_deref().unwrap_or(&name).to_string();
                return Err(Problem {
                    message: msg![code(&c_name), " only exists on the CPU"],
                    span: self.span.clone(),
                    label: msg![code(format!("{name}(…)")), " is called here"],
                    hint: Some(msg![
                        "a shader calls GLSL's functions instead, from ",
                        code("som::gl"),
                        ", inside ",
                        code("@if GPU { … }")
                    ]),
                });
            }
        };
        // What it takes and gives has to be on the GPU: said at the call, not inside it.
        let on_gpu = |t: &mut Translator, ty: TypeId| -> Result<(), NoGpu> {
            let ty = match t.mir.types.get(ty) {
                Ty::Ref { to, .. } => *to,
                _ => ty,
            };
            t.ty(ty).map(|_| ())
        };
        for &param in &callee.params {
            on_gpu(t, param).map_err(|no| {
                let what = msg![code(format!("{name}(…)")), " takes a ", t.ty_part(param)];
                self.no_gpu(no, Some(what))
            })?;
        }
        if matches!(t.mir.types.get(callee.ret), Ty::Ref { .. }) {
            return Err(Problem {
                message: msg!["a shader can't use the reference ", code(&name), " returns"],
                span: self.span.clone(),
                label: msg![code(format!("{name}(…)")), " is called here"],
                hint: Some("on the GPU, references only go into calls: return a copy".into()),
            });
        }
        on_gpu(t, callee.ret).map_err(|no| {
            let what = msg![code(format!("{name}(…)")), " returns a ", t.ty_part(callee.ret)];
            self.no_gpu(no, Some(what))
        })?;

        let given: Vec<Operand> = (args.iter())
            .zip(&callee.params)
            .filter(|(_, p)| !t.is_unit(**p))
            .map(|(op, _)| op.clone())
            .collect();
        let texts = (given.iter())
            .map(|op| self.operand(t, op))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(match builtin {
            Some(glsl) => {
                // GLSL's `atan` of two, which `som::gl` names apart: externs can't overload.
                let glsl = if glsl == "atan2" { "atan" } else { glsl.as_str() };
                format!("{glsl}({})", texts.join(", "))
            }
            None => match vector_op(t, func, &texts) {
                Some(native) => native,
                None => {
                    t.reach(self.func, func, &self.span);
                    format!("{}({})", t.func_name(func), texts.join(", "))
                }
            },
        })
    }
}

/// A call of the standard library's `Vector` operations, as GLSL's own: `a + b`, `v * 2.0`,
/// `v.xy`, `length(v)`. `None` for any other function.
fn vector_op(t: &Translator, func: FuncId, args: &[String]) -> Option<String> {
    let callee = &t.mir.funcs[func];
    // `vec4(rgb, 1.0)`: GLSL's constructors take the same.
    for n in ["vec2", "vec3", "vec4"] {
        if callee.name == format!("som::data::{n}") {
            return Some(format!("{n}({})", args.join(", ")));
        }
    }
    // `0.5 * v`: a number times a vector, GLSL's own `*` too.
    if callee.name.starts_with("som::data::f32::multiply<")
        && let [a, b] = args
        && callee.params.get(1).is_some_and(|&p| {
            let of = match t.mir.types.get(p) {
                Ty::Ref { to, .. } => *to,
                _ => p,
            };
            t.vector(of).is_some()
        })
    {
        return Some(format!("({a} * {b})"));
    }
    let rest = callee.name.strip_prefix("som::data::Vector::")?;
    let op = rest.split('<').next()?;
    let first = *callee.params.first()?;
    let of = match t.mir.types.get(first) {
        Ty::Ref { to, .. } => *to,
        _ => first,
    };
    // `Vector::new`'s argument is the array, which is already the vector.
    if op == "new" {
        return t.native_vector(of).map(|_| args[0].clone());
    }
    t.vector(of)?;
    let a = args.first()?;
    Some(match (op, &args[1..]) {
        ("add", [b]) => format!("({a} + {b})"),
        ("subtract", [b]) => format!("({a} - {b})"),
        ("multiply", [b]) => format!("({a} * {b})"),
        ("divide", [b]) => format!("({a} / {b})"),
        ("x" | "y" | "z" | "w" | "xy" | "yz" | "xz" | "xyz", []) => format!("{a}.{op}"),
        ("r" | "g" | "b" | "a" | "rgb", []) => format!("{a}.{op}"),
        ("magnitude" | "length", []) => format!("length({a})"),
        ("normalized", []) => format!("normalize({a})"),
        ("abs", []) => format!("abs({a})"),
        ("negate", []) => format!("(-{a})"),
        ("dot" | "min" | "max", [b]) => format!("{op}({a}, {b})"),
        ("mix" | "clamp", [b, c]) => format!("{op}({a}, {b}, {c})"),
        _ => return None,
    })
}

fn binary(op: BinOp, scalar: Option<Scalar>, a: &str, b: &str) -> String {
    let symbol = match (op, scalar) {
        (BinOp::Add, _) => "+",
        (BinOp::Subtract, _) => "-",
        (BinOp::Multiply, _) => "*",
        (BinOp::Divide, _) => "/",
        // C's remainder: its sign is what's divided's. GLSL's `%` leaves negative ones open,
        // and floats have `mod`, whose sign is the divisor's.
        (BinOp::Modulo, Some(Scalar::Float)) => {
            return format!("({a} - {b} * trunc({a} / {b}))");
        }
        (BinOp::Modulo, Some(Scalar::Int)) => return format!("({a} - {b} * ({a} / {b}))"),
        (BinOp::Modulo, _) => "%",
        (BinOp::LessThan, _) => "<",
        (BinOp::GreaterThan, _) => ">",
        (BinOp::LessThanOrEqual, _) => "<=",
        (BinOp::GreaterThanOrEqual, _) => ">=",
        (BinOp::Equals, _) => "==",
        (BinOp::NotEquals, _) => "!=",
        (BinOp::And, _) | (BinOp::BitAnd, Some(Scalar::Bool)) => "&&",
        (BinOp::Or, _) | (BinOp::BitOr, Some(Scalar::Bool)) => "||",
        (BinOp::BitXor, Some(Scalar::Bool)) => "^^",
        (BinOp::BitAnd, _) => "&",
        (BinOp::BitOr, _) => "|",
        (BinOp::BitXor, _) => "^",
        (BinOp::ShiftLeft, _) => "<<",
        (BinOp::ShiftRight, _) => ">>",
    };
    format!("({a} {symbol} {b})")
}
