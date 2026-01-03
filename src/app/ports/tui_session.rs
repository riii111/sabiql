use color_eyre::eyre::Result;

pub trait TuiSession {
    fn suspend(&mut self) -> Result<()>;
    fn resume(&mut self) -> Result<()>;
}
