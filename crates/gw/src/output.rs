/// The outcome of a command, mapped to an exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    pub code: u8,
}

impl Outcome {
    pub fn ok() -> Self {
        Self { code: 0 }
    }

    pub fn partial() -> Self {
        Self { code: 3 }
    }

    pub fn code(self) -> u8 {
        self.code
    }
}
