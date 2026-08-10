pub mod consts;
pub mod utils;

#[derive(Debug, Clone)]
pub struct Info {
    pub display_width: usize,
    pub display_height: usize,
    pub chunks_per_x: usize,
    pub chunks_per_y: usize,
    pub thread_count: usize,
    pub fps: f64,
}

impl Info {
    pub fn display_size(&self) -> usize {
        self.display_width * self.display_height
    }

    pub fn chunk_width(&self) -> usize {
        self.display_width / self.chunks_per_x
    }

    pub fn chunk_height(&self) -> usize {
        self.display_height / self.chunks_per_y
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks_per_x * self.chunks_per_y
    }

    pub fn chunk_size(&self) -> usize {
        self.chunk_width() * self.chunk_height()
    }
}
