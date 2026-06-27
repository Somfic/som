#[derive(Debug)]
pub struct Scope<V> {
    stack: Vec<(Box<str>, V)>,
}

#[derive(Debug, Clone, Copy)]
pub struct ScopeMark(usize);

impl<V> Scope<V> {
    pub fn new() -> Self {
        Self { stack: Vec::new() }
    }

    pub fn define(&mut self, name: impl Into<Box<str>>, value: V) {
        self.stack.push((name.into(), value));
    }

    pub fn lookup(&self, name: &str) -> Option<&V> {
        self.stack
            .iter()
            .rev()
            .find(|(n, _)| &**n == name)
            .map(|(_, v)| v)
    }

    pub fn enter(&self) -> ScopeMark {
        ScopeMark(self.stack.len())
    }

    pub fn exit(&mut self, mark: ScopeMark) {
        self.stack.truncate(mark.0);
    }
}

impl<V> Default for Scope<V> {
    fn default() -> Self {
        Self::new()
    }
}
