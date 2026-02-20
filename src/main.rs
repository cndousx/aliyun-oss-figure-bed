use aliyun_oss_client::Client;
use chrono::Local;
use futures::stream::{self, StreamExt};
use std::env;
use std::path::Path;
use uuid::Uuid;

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    let is_md = &args[1] == "md";
    let start = if is_md { 2 } else { 1 };

    for i in start..args.len() {
        let file = &args[i];
        if !Path::new(&file).exists() {
            panic!("文件 {} 不存在", file);
        }
        if let None = Path::new(&file).extension() {
            panic!("文件 {} 没有扩展名", file);
        }
    }
    let client = Client::from_env().unwrap();
    let buckets = client.get_buckets().await.unwrap();
    if buckets.len() != 1 {
        panic!("Expected exactly one bucket");
    }

    let bucket = buckets.get(0).unwrap();
    let bucket_url = bucket.to_url().unwrap().as_str().to_string();

    let tasks: Vec<_> = (start..args.len())
        .map(|i| {
            let arg = &args[i];
            let ext = match Path::new(arg).extension() {
                None => {
                    panic!();
                }
                Some(ext) => ext.to_string_lossy().to_string(),
            };
            let timestamp = Local::now().format("%Y/%m/%d/%H-%M-%S-%3f").to_string();
            let uuid_simple = Uuid::new_v4().simple();
            let fs = format!("{timestamp}-{uuid_simple}.{ext}");
            let md = format!("markdown/{fs}");
            let url = format!("{}{}", bucket_url, md);

            async move {
                match tokio::fs::File::open(arg).await {
                    Ok(file) => match &bucket.object(&md).upload(file).await {
                        Ok(_) => {
                            if is_md {
                                Some(format!("![{timestamp}]({url})"))
                            } else {
                                Some(url)
                            }
                        }
                        Err(e) => {
                            eprintln!("上传异常: {} {}", arg, e);
                            None
                        }
                    },
                    Err(e) => {
                        eprintln!("文件打开失败: {} {}", arg, e);
                        None
                    }
                }
            }
        })
        .collect();

    let max_concurrent = std::env::var("ALIYUN-OSS-FIGURE-BED-MAX-CONCURRENT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3); // 默认并发数为3

    // 使用 stream 控制并发数
    let results: Vec<_> = stream::iter(tasks)
        .buffer_unordered(max_concurrent) // 最多同时运行max_concurrent个任务
        .collect()
        .await;

    for result in results {
        match result {
            Some(output) => println!("{}", output),
            None => {}
        }
    }
}
