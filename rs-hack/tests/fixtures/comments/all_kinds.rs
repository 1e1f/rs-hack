//! Module docs for the fixture.
//! Second inner-doc line.

/*! Inner block doc. */

// @yah:ticket(X1-T1, "fixture annotation")
// @yah:next("wrapped value that
// continues on this line")
// plain prose after the annotation

/// Doc on a struct.
/// Two lines.
#[derive(Debug)]
pub struct Widget {
    /// Doc on a field.
    pub size: u32, // trailing line comment
}

/** Block doc on a fn. */
pub fn run(w: &Widget) -> u32 {
    // body comment one
    // body comment two
    let s = "not // a comment";
    let r = r#"nor /* this */ one"#;
    let c = '/';
    let q = ['\'', '"', b'"' as char]; // after tricky quotes
    let _ = q;
    let lt: &'static str = "x";
    /* body block /* nested */ still block */
    w.size + s.len() as u32 + r.len() as u32 + c as u32 + lt.len() as u32
}

//// four slashes is a plain line comment
/*** three stars is a plain block comment */
impl Widget {
    // regular comment attached to a method
    fn helper(&self) -> u32 {
        self.size
    }
}
