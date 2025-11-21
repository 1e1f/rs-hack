// rshack is a convenient alias for rs-hack (no hyphens!)
// This package provides the same CLI as rs-hack, but with a simpler name to type

// Re-export everything from rs-hack
pub use rs_hack::*;

// Import the main implementation from rs-hack's binary
// Since we can't directly call another crate's main(), we need to implement it here
// For now, we'll just tell users to use rs-hack until we properly integrate this

fn main() {
    eprintln!("Note: rshack is temporarily unavailable in v0.5.1");
    eprintln!("Please use 'rs-hack' instead, or install from an earlier version.");
    eprintln!("We're working on fixing the rshack alias for the next release!");
    std::process::exit(1);
}
