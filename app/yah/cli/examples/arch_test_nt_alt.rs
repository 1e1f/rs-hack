use yah::arch::extract::extract_from_workspace;
use yah::arch::graph::ArchGraph;
use yah::arch::query::{get_file_context, Query};

fn main() {
    let path = "/Users/leif/ss/nt_alt/crates/vivarium";
    
    println!("Extracting from {}...\n", path);
    let annotations = extract_from_workspace(path).expect("Failed to extract");
    println!("Found {} annotations\n", annotations.len());
    
    let graph = ArchGraph::from_annotations(annotations);
    
    // Query for vivarium layer
    let q = Query::parse("layer:vivarium").unwrap();
    let result = q.execute(&graph);
    println!("=== Nodes in layer:vivarium ===");
    for id in &result.nodes {
        println!("  {}", id);
    }
    println!("\n{} nodes total\n", result.count);
    
    // Get context for banana
    println!("=== Context for banana ===");
    let ctx = get_file_context(&graph, "banana/src/lib.rs");
    println!("{}", ctx.to_markdown("banana/src/lib.rs"));
    
    // Get context for impulse
    println!("\n=== Context for impulse ===");
    let ctx = get_file_context(&graph, "impulse/src/lib.rs");
    println!("{}", ctx.to_markdown("impulse/src/lib.rs"));
}
