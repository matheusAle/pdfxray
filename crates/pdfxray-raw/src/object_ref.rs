use std::fmt;

/// An indirect object reference `num gen R`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct ObjRef {
    /// Object number.
    pub num: u32,
    /// Generation number.
    pub generation: u16,
}

impl ObjRef {
    /// Creates a reference.
    pub const fn new(num: u32, generation: u16) -> Self {
        Self { num, generation }
    }
}

impl fmt::Display for ObjRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} R", self.num, self.generation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displays_as_pdf_reference_syntax() {
        assert_eq!(ObjRef::new(12, 0).to_string(), "12 0 R");
        assert!(ObjRef::new(1, 0) < ObjRef::new(1, 1));
    }
}
