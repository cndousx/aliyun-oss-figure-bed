use aliyun_oss_client::Client;
use chrono::Local;
use futures::stream::{self, StreamExt};
use std::env;
use std::path::Path;
use uuid::Uuid;

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    let (out_md, uploads) = parse_args(&args);

    let client = Client::from_env().unwrap();
    let buckets = client.get_buckets().await.unwrap();
    if buckets.len() != 1 {
        panic!("Expected exactly one bucket");
    }

    let bucket = buckets.get(0).unwrap();
    let bucket_url = bucket.to_url().unwrap().as_str().to_string();

    let tasks = uploads
        .iter()
        .map(|(arg, ext)| {
            let timestamp = Local::now().format("%Y/%m/%d/%H-%M-%S-%3f").to_string();
            let uuid_simple = Uuid::new_v4().simple();
            let fs = format!("{timestamp}-{uuid_simple}.{ext}");
            let md = format!("markdown/{fs}");
            let url = format!("{}{}", bucket_url, md);

            async move {
                match tokio::fs::File::open(arg).await {
                    Ok(file) => match &bucket.object(&md).upload(file).await {
                        Ok(_) => {
                            if out_md {
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
        .collect::<Vec<_>>();

    // 使用 stream 控制并发数
    let results = stream::iter(tasks)
        .buffer_unordered(max_concurrent())
        .collect::<Vec<_>>()
        .await;

    for result in results {
        match result {
            Some(output) => println!("{}", output),
            None => {}
        }
    }
}

/// 获取最大并发数
fn max_concurrent() -> usize {
    std::env::var("ALIYUN-OSS-FIGURE-BED-MAX-CONCURRENT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1) as usize // 默认并发数为1
}
/// 解析参数
fn parse_args(args: &Vec<String>) -> (bool, Vec<(&String, String)>) {
    let (out_md, start) = validate_args(&args).unwrap();
    let uploads = (start..args.len())
        .map(|i| {
            let arg = &args[i];
            let file = Path::new(&arg);
            if !file.exists() {
                panic!("文件不存在：{}", arg);
            }
            if let None = file.extension() {
                panic!("文件没有扩展名：{}", arg);
            }
            let ext = match file.extension() {
                None => {
                    panic!("文件没有扩展名：{}", arg);
                }
                Some(ext) => ext.to_string_lossy().to_string(),
            };
            (arg, ext)
        })
        .collect::<Vec<_>>();
    (out_md, uploads)
}
/// 校验参数
fn validate_args(args: &[String]) -> Result<(bool, usize), String> {
    if args.len() < 2 {
        return Err("至少需要一个文件参数".to_string());
    }

    let is_md = &args[1] == "md";
    let start = if is_md { 2 } else { 1 };

    if start >= args.len() {
        return Err("没有提供文件参数".to_string());
    }

    Ok((is_md, start))
}
