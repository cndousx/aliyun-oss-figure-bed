use aliyun_oss_client::{Bucket, Client};
use anyhow::{Result, anyhow};
use chrono::Local;
use futures::stream::{self, StreamExt};
use mime_guess::from_ext;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<()> {
    // 解析参数
    let args: Vec<String> = env::args().collect();
    let (out_md, uploads) = parse_args(&args)?;

    // 根据环境变量构建oss bucket
    let client = Client::from_env().map_err(|e| anyhow!("初始化 OSS Client 失败: {}", e))?;
    let bucket = Arc::new(oss_bucket(&client).await?);
    let url = bucket.to_url()?;

    let tasks = create_upload_future(uploads, bucket, url.as_str(), out_md);

    execute_upload_tasks(tasks).await?;

    Ok(())
}

/// 执行所有上传任务
async fn execute_upload_tasks(
    tasks: Vec<impl Future<Output = Option<String>> + Send>,
) -> Result<()> {
    let results = stream::iter(tasks)
        .buffer_unordered(max_concurrent())
        .collect::<Vec<_>>()
        .await;

    for result in results {
        if let Some(output) = result {
            println!("{}", output);
        }
    }

    Ok(())
}
/// 构建上传任务
fn create_upload_future(
    uploads: Vec<(PathBuf, String)>,
    bucket: Arc<Bucket>,
    bucket_url: &str,
    out_md: bool,
) -> Vec<impl Future<Output = Option<String>>> {
    uploads
        .into_iter()
        .map(move |(path, ext)| {
            let bucket = Arc::clone(&bucket);
            let out_md = out_md;
            let timestamp = Local::now().format("%Y/%m/%d/%H-%M-%S-%3f").to_string();
            // 使用当前时间生成 UUID v7
            let uid = Uuid::now_v7();
            let filename = format!("{timestamp}-{uid}.{ext}");
            let key = format!("markdown/{filename}");
            let url = format!("{}{}", bucket_url, key);

            async move {
                match upload_file(&bucket, &key, &path, &ext).await {
                    Ok(_) => {
                        if out_md {
                            Some(format!("![{timestamp}]({url})"))
                        } else {
                            Some(url)
                        }
                    }
                    Err(e) => {
                        eprintln!("上传失败 {}: {}", path.display(), e);
                        None
                    }
                }
            }
        })
        .collect::<Vec<_>>()
}
async fn oss_bucket(client: &Client) -> Result<Bucket> {
    let buckets = client.get_buckets().await?;
    if buckets.len() != 1 {
        anyhow::bail!("OSS Bucket err")
    } else {
        buckets
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("未找到可用的 OSS Bucket"))
    }
}

async fn upload_file(bucket: &Arc<Bucket>, key: &str, path: &Path, ext: &str) -> Result<()> {
    let file = tokio::fs::File::open(path).await?;
    let mut object = bucket.object(key);

    // 设置正确的 Content-Type
    if let Some(mime) = from_ext(ext).first() {
        object = object.content_type_mime(mime);
    }

    object.upload(file).await?;
    Ok(())
}

fn max_concurrent() -> usize {
    env::var("ALIYUN_OSS_FIGURE_BED_MAX_CONCURRENT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3) // 默认并发数 3，可通过环境变量调整
}

fn parse_args(args: &[String]) -> Result<(bool, Vec<(PathBuf, String)>)> {
    if args.len() < 2 {
        anyhow::bail!(
            "缺少参数，用法示例: {} [md] <file1> [file2 ...]",
            args.first().map_or("oss-upload", |s| s.as_str())
        );
    }
    // 是否以md格式输出结果
    // true 则输出 [name](url)
    // false 仅输出url
    let out_md = args.get(1).map_or(false, |s| s == "md");
    let start = if out_md { 2 } else { 1 };

    if start >= args.len() {
        anyhow::bail!("没有提供文件参数");
    }

    let uploads = (start..args.len())
        .map(|i| {
            let path = PathBuf::from(&args[i]);
            if !path.exists() {
                anyhow::bail!("文件不存在: {}", path.display());
            }
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .ok_or_else(|| anyhow!("文件没有扩展名: {}", path.display()))?
                .to_lowercase();

            Ok((path, ext))
        })
        .collect::<Result<Vec<_>>>()?;

    Ok((out_md, uploads))
}
