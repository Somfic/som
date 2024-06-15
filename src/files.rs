use std::collections::HashMap;

#[derive(Default)]
pub struct Files<'a> {
    pub files: HashMap<&'a str, &'a str>,
}

impl<'a> Files<'a> {
    pub fn insert(&mut self, file_id: impl Into<&'a str>, source: impl Into<&'a str>) {
        self.files.insert(file_id.into(), source.into());
    }

    pub fn file_ids(&self) -> Vec<&'a str> {
        self.files.keys().copied().collect()
    }

    pub fn get(&self, file_id: impl Into<&'a str>) -> Option<&'a str> {
        self.files.get(file_id.into()).copied()
    }
}

impl<'a> codespan_reporting::files::Files<'a> for Files<'a> {
    type FileId = &'a str;
    type Name = &'a str;
    type Source = &'a str;

    fn name(&'a self, id: Self::FileId) -> Result<Self::Name, codespan_reporting::files::Error> {
        self.files
            .keys()
            .find(|key| **key == id)
            .copied()
            .ok_or(codespan_reporting::files::Error::FileMissing)
    }

    fn source(
        &'a self,
        id: Self::FileId,
    ) -> Result<Self::Source, codespan_reporting::files::Error> {
        self.files
            .get(id)
            .ok_or(codespan_reporting::files::Error::FileMissing)
            .copied()
    }

    fn line_index(
        &'a self,
        id: Self::FileId,
        byte_index: usize,
    ) -> Result<usize, codespan_reporting::files::Error> {
        self.get(id)
            .ok_or(codespan_reporting::files::Error::FileMissing)
            .map(|source| {
                source
                    .char_indices()
                    .take_while(|(index, _)| *index < byte_index)
                    .filter(|(_, character)| *character == '\n')
                    .count()
            })
    }

    fn line_range(
        &'a self,
        id: Self::FileId,
        line_index: usize,
    ) -> Result<std::ops::Range<usize>, codespan_reporting::files::Error> {
        self.get(id)
            .ok_or(codespan_reporting::files::Error::FileMissing)
            .map(|source| {
                let start = source
                    .lines()
                    .take(line_index)
                    .map(|line| line.len() + 1)
                    .sum::<usize>();

                let end = source
                    .lines()
                    .take(line_index + 1)
                    .map(|line| line.len() + 1)
                    .sum::<usize>();

                start..end
            })
    }
}
