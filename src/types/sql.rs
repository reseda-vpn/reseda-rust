#[derive(sqlx::FromRow, Debug)]
pub struct ReturnType {
    tier: String
}