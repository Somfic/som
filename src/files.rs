use std::collections::HashMap;

use codespan_reporting::files::SimpleFiles;

pub struct Files<'a> {
    files: SimpleFiles<&'a str, &'a str>,
    file_handles: HashMap<&'a str, usize>,
}

impl<'a> Default for Files<'a> {
    fn default() -> Self {
        Self {
            files: SimpleFiles::new(),
            file_handles: HashMap::new(),
        }
    }
}

impl<'a> Files<'a> {
    pub fn insert(&mut self, file_id: &'a str, source: &'a str) {
        let handle = self.files.add(source, source);
        self.file_handles.insert(file_id, handle);
    }

    pub fn get(&self, file_id: impl Into<&'a str>) -> Option<&'a str> {
        let handle = self.file_handles.get(file_id.into())?;
        Some(self.files.get(*handle).unwrap().source())
    }

    pub fn file_ids<'b>(&'b self) -> impl Iterator<Item = &'a str> + 'b {
        self.file_handles.keys().copied()
    }
}

impl<'a> codespan_reporting::files::Files<'a> for Files<'a> {
    type FileId = &'a str;
    type Name = &'a str;
    type Source = &'a str;

    fn name(&'a self, id: Self::FileId) -> Result<Self::Name, codespan_reporting::files::Error> {
        Ok(id)
    }

    fn source(
        &'a self,
        id: Self::FileId,
    ) -> Result<Self::Source, codespan_reporting::files::Error> {
        self.get(id)
            .ok_or(codespan_reporting::files::Error::FileMissing)
    }

    fn line_index(
        &'a self,
        id: Self::FileId,
        byte_index: usize,
    ) -> Result<usize, codespan_reporting::files::Error> {
        let handle = self
            .file_handles
            .get(id)
            .ok_or(codespan_reporting::files::Error::FileMissing)?;

        self.files.line_index(*handle, byte_index)
    }

    fn line_range(
        &'a self,
        id: Self::FileId,
        line_index: usize,
    ) -> Result<std::ops::Range<usize>, codespan_reporting::files::Error> {
        let handle = self
            .file_handles
            .get(id)
            .ok_or(codespan_reporting::files::Error::FileMissing)?;

        self.files.line_range(*handle, line_index)
    }
}
