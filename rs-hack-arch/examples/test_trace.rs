use rs_hack_arch::extract::extract_from_workspace;
use rs_hack_arch::graph::ArchGraph;
use rs_hack_arch::query::trace_path;

fn main() {
    let path = "/Users/leif/ss/nt_alt/crates/vivarium";
    
    let annotations = extract_from_workspace(path).expect("Failed to extract");
    let graph = ArchGraph::from_annotations(annotations);
    
    println!("=== Tracing: gateway → role:synthesis ===\n");
    
    match trace_path(&graph, "gateway", "role:synthesis") {
        Ok(paths) => {
            if paths.is_empty() {
                println!("No paths found");
            } else {
                for (i, path) in paths.iter().enumerate() {
                    println!("Path {}:", i + 1);
                    for (j, node) in path.iter().enumerate() {
                        let prefix = if j == 0 { "  " } else { "  ↓ " };
                        // Shorten the path for readability
                        let short = node.split("::").last().unwrap_or(node);
                        println!("{}{}", prefix, short);
                    }
                    println!();
                }
            }
        }
        Err(e) => println!("Error: {}", e),
    }
    
    println!("=== Graph edges ===\n");
    for edge in graph.edges() {
        let from_short = edge.from.split("::").last().unwrap_or(&edge.from);
        let to_short = edge.to.split("::").last().unwrap_or(&edge.to);
        println!("{} --[{:?}]--> {}", from_short, edge.kind, to_short);
        if let Some(ref reason) = edge.reason {
            println!("  reason: {}", reason);
        }
    }
}
