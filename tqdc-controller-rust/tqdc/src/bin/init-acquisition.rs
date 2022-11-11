#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let point = tqdc::acquire_point(1).await?;
    println!("{point:?}");
    Ok(())
}