

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let data_from_tqdc = tqdc::acquire_point(10).await?;

    let hi_ch = data_from_tqdc.iter().map(|header| {
        header.hi_ch
    });

    let hi_min = hi_ch.clone().min().unwrap();
    let hi_max = hi_ch.clone().max().unwrap();
    let perc = (hi_max - hi_min) as f32 / hi_max as f32;

    println!("{hi_min} {hi_max} {perc}");

    let trace = plotly::Histogram::new(hi_ch.collect()).name("hi_ch");

    let mut plot = plotly::Plot::new();
    plot.add_trace(trace);
    plot.show();
    
    Ok(())
}