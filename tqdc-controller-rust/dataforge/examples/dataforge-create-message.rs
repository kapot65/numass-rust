use dataforge::write_df_message;

#[tokio::main]
async fn main() {

    let data = r#"
        {
            "type": "point"
        }"#;

        // Parse the string of data into serde_json::Value.
        let meta: serde_json::Value = serde_json::from_str(data).unwrap();

        let mut stream = vec![];
        write_df_message(&mut stream, meta, None).await.unwrap();

        println!("{:?}", String::from_utf8_lossy(&stream))
}