use helheim_lang::ast::CodeTaal;
use crate::common::context::ExecutionContext;

pub enum EvalFrame {
    Statements {
        statements: Vec<CodeTaal>,
        pc: usize,
        ctx: ExecutionContext,
    },
}

pub struct TrampolineStack {
    pub frames: Vec<EvalFrame>,
}

impl TrampolineStack {
    pub fn new() -> Self {
        Self { frames: Vec::new() }
    }

    pub fn push(&mut self, frame: EvalFrame) -> anyhow::Result<()> {
        if self.frames.len() >= 10_000 {
            return Err(anyhow::anyhow!("ExecutionError::StackOverflow"));
        }
        self.frames.push(frame);
        Ok(())
    }

    pub fn pop(&mut self) -> Option<EvalFrame> {
        self.frames.pop()
    }
}
