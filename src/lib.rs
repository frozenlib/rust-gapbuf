#![feature(generators, generator_trait)]
#![feature(test)]
#![feature(crate_in_paths)]

#[macro_use]
mod finally;

#[macro_use]
mod gap_buffer;
mod raw_buffer;
mod tests;

pub use gap_buffer::GapBuffer;
