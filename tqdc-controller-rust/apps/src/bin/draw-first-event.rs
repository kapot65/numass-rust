use plotly::{Plot, Scatter};
use dataforge::protos::rsb_event;
use protobuf::Message;

#[tokio::main]
async fn main() {

    let mut point_file = tokio::fs::File::open(
        // "./test-data/points/p-2022-11-12-virtual.df"
        "./test-data/points/p-2022-11-14-real-5v.df"
        // "./test-data/points/p0(30s)(HV1=14000).df"
    ).await.unwrap();
    let message = dataforge::extract_df_message(&mut point_file).await.unwrap();

    let data = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();

    let mut plot = Plot::new();

    for channel in &data.channels {

        println!("{}", channel.id);
        for block in &channel.blocks {
            for frame in &block.frames {

                let waveform_len = frame.data.len() / 2;
        
                let mut waveform = Vec::with_capacity(waveform_len);
                for idx in 0..waveform_len {
                    waveform.push(i16::from_le_bytes(frame.data[idx*2..idx*2+2].try_into().unwrap()) as i32)
                }


                // println!("{:?}", waveform);

                // let a: Vec<_> = (0..10).collect();
                
                let trace = Scatter::new((0..waveform_len).collect(), waveform);
                plot.add_trace(trace);

                
                break;

                // frame.data
            }
        }
    }

    plot.show();
    // return;

    // println!("{:?}", data);
    
}