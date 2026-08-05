/// A transform rule that applies a math operation to a source field value
/// and stores the result under a target field name.
#[derive(Debug, Clone)]
pub struct TransformRule {
    pub source: String,
    pub op: String, // "multiply" | "add" | "divide" | "subtract"
    pub factor: f64,
    pub target: String,
}
