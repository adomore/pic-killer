//! 命令行参数定义（子命令架构）。

use clap::{ArgGroup, Args, Parser, Subcommand};
use std::path::PathBuf;

/// PIC-Killer —— 照片元数据瑞士军刀
///
/// 无损批量修改照片的 EXIF 元数据：拍摄时间、作者版权、相机镜头、GPS 定位、方向，
/// 以及查看与清除。只改写元数据段，不重新编码图像，像素数据完全无损。
/// 支持 JPEG / PNG / TIFF / WebP / HEIC / AVIF / JXL。
#[derive(Parser, Debug)]
#[command(name = "pic-killer", version, about = "照片元数据瑞士军刀（无损批量修改）")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// 修改拍摄时间：固定值 / 相对偏移 / 序列递增 / 从文件名提取
    Time(TimeArgs),
    /// 查看或导出元数据（只读）
    Show(ShowArgs),
    /// 设置常见标签：作者、版权、描述、相机、镜头、方向等
    Set(SetArgs),
    /// 设置或清除 GPS 定位
    Gps(GpsArgs),
    /// 清除元数据（保护隐私）：全部 / 仅 GPS
    Strip(StripArgs),
    /// 无损旋转标记：在现有方向基础上叠加旋转/镜像
    Rotate(RotateArgs),
    /// 从参考照片复制元数据到一批照片
    Copy(CopyArgs),
    /// 按拍摄时间批量重命名文件
    Rename(RenameArgs),
    /// 读写 XMP 元数据（标题/描述/作者/评分/关键词等，JPEG 与 PNG）
    Xmp(XmpArgs),
    /// 读写旧版 IPTC-IIM 元数据（APP13/8BIM，仅 JPEG）
    Iptc(IptcArgs),
}

/// 选择要处理的文件（所有子命令共用）。
#[derive(Args, Debug)]
pub struct TargetArgs {
    /// 待处理的文件或目录（可指定多个）
    #[arg(required = true, value_name = "路径")]
    pub paths: Vec<PathBuf>,

    /// 递归处理子目录
    #[arg(short, long)]
    pub recursive: bool,

    /// 要处理的扩展名，逗号分隔
    #[arg(long, value_name = "列表", default_value = "jpg,jpeg,png,tif,tiff,webp")]
    pub ext: String,
}

/// 写入类命令共用的行为开关。
#[derive(Args, Debug)]
pub struct WriteArgs {
    /// 处理前将原文件备份为 <文件名>.bak
    #[arg(long)]
    pub backup: bool,

    /// 仅预览将要做的修改，不实际写入
    #[arg(short = 'n', long)]
    pub dry_run: bool,

    /// 跳过写入前的确认提示
    #[arg(short = 'y', long)]
    pub yes: bool,

    /// 输出更详细的信息
    #[arg(short, long)]
    pub verbose: bool,
}

// ----------------------------- time -----------------------------

#[derive(Args, Debug)]
#[command(group(
    ArgGroup::new("op")
        .required(true)
        .multiple(false)
        .args(["set", "shift", "sequential", "from_name"])
))]
pub struct TimeArgs {
    #[command(flatten)]
    pub target: TargetArgs,
    #[command(flatten)]
    pub write: WriteArgs,

    /// 设为指定绝对时间，如 "2024-01-01 12:00:00"
    #[arg(long, value_name = "时间")]
    pub set: Option<String>,

    /// 在原时间上相对偏移，如 +2h、-3d、+1y2mo、-30m、+1d12h30m
    #[arg(long, value_name = "偏移", allow_hyphen_values = true)]
    pub shift: Option<String>,

    /// 序列模式：给首张设定起始时间，其余按 --interval 依次递增
    #[arg(long, value_name = "起始时间")]
    pub sequential: Option<String>,

    /// 序列模式每张之间的间隔（默认 +1s）
    #[arg(long, value_name = "偏移", default_value = "+1s", allow_hyphen_values = true)]
    pub interval: String,

    /// 从文件名中提取日期时间（如 IMG_20230115_143022.jpg）
    #[arg(long)]
    pub from_name: bool,

    /// 写入的时间字段，逗号分隔：original,digitized,modify（默认全部）
    #[arg(long, value_name = "列表", default_value = "original,digitized,modify")]
    pub tags: String,

    /// 同时把文件系统修改时间也设为该拍摄时间
    #[arg(long)]
    pub also_file_time: bool,
}

// ----------------------------- show -----------------------------

#[derive(Args, Debug)]
pub struct ShowArgs {
    #[command(flatten)]
    pub target: TargetArgs,

    /// 以 JSON 格式输出（便于脚本处理）
    #[arg(long, conflicts_with = "csv")]
    pub json: bool,

    /// 以 CSV 格式输出（每个标签一行：file,group,name,hex,value）
    #[arg(long)]
    pub csv: bool,

    /// 只显示名称含该关键字的标签（大小写不敏感）
    #[arg(long, value_name = "关键字")]
    pub filter: Option<String>,
}

// ----------------------------- set ------------------------------

#[derive(Args, Debug)]
pub struct SetArgs {
    #[command(flatten)]
    pub target: TargetArgs,
    #[command(flatten)]
    pub write: WriteArgs,

    /// 作者 / 摄影师（Artist）
    #[arg(long, value_name = "文本")]
    pub artist: Option<String>,
    /// 版权信息（Copyright）
    #[arg(long, value_name = "文本")]
    pub copyright: Option<String>,
    /// 图像描述 / 标题（ImageDescription）
    #[arg(long, value_name = "文本")]
    pub description: Option<String>,
    /// 处理软件（Software）
    #[arg(long, value_name = "文本")]
    pub software: Option<String>,
    /// 相机厂商（Make）
    #[arg(long, value_name = "文本")]
    pub make: Option<String>,
    /// 相机型号（Model）
    #[arg(long, value_name = "文本")]
    pub model: Option<String>,
    /// 镜头型号（LensModel）
    #[arg(long, value_name = "文本")]
    pub lens_model: Option<String>,
    /// 用户注释（UserComment）
    #[arg(long, value_name = "文本")]
    pub user_comment: Option<String>,
    /// 拥有者（OwnerName）
    #[arg(long, value_name = "文本")]
    pub owner: Option<String>,

    /// 方向：normal / cw90 / ccw90 / 180 / mirror-h / mirror-v，或 1-8
    #[arg(long, value_name = "方向")]
    pub orientation: Option<String>,

    /// 通用设置任意字符串标签，格式 名称=值（可重复）
    #[arg(long = "set-string", value_name = "名称=值")]
    pub set_string: Vec<String>,

    /// 删除指定标签，按名称（可重复）
    #[arg(long = "remove", value_name = "名称")]
    pub remove: Vec<String>,
}

// ----------------------------- gps ------------------------------

#[derive(Args, Debug)]
#[command(group(
    ArgGroup::new("gps_op").required(true).multiple(true).args(["lat", "clear"])
))]
pub struct GpsArgs {
    #[command(flatten)]
    pub target: TargetArgs,
    #[command(flatten)]
    pub write: WriteArgs,

    /// 纬度（十进制度，北纬为正、南纬为负）
    #[arg(long, value_name = "度", allow_hyphen_values = true, requires = "lon")]
    pub lat: Option<f64>,

    /// 经度（十进制度，东经为正、西经为负）
    #[arg(long, value_name = "度", allow_hyphen_values = true, requires = "lat")]
    pub lon: Option<f64>,

    /// 海拔（米，可选）
    #[arg(long, value_name = "米", allow_hyphen_values = true)]
    pub alt: Option<f64>,

    /// 清除所有 GPS 信息
    #[arg(long)]
    pub clear: bool,
}

// ---------------------------- strip -----------------------------

#[derive(Args, Debug)]
pub struct StripArgs {
    #[command(flatten)]
    pub target: TargetArgs,
    #[command(flatten)]
    pub write: WriteArgs,

    /// 只清除 GPS 定位（默认清除全部元数据）
    #[arg(long)]
    pub gps: bool,
}

// ---------------------------- rotate ----------------------------

#[derive(Args, Debug)]
#[command(group(
    ArgGroup::new("rot")
        .required(true)
        .multiple(false)
        .args(["cw", "ccw", "r180", "flip_h", "flip_v", "reset"])
))]
pub struct RotateArgs {
    #[command(flatten)]
    pub target: TargetArgs,
    #[command(flatten)]
    pub write: WriteArgs,

    /// 顺时针旋转 90°
    #[arg(long)]
    pub cw: bool,
    /// 逆时针旋转 90°
    #[arg(long)]
    pub ccw: bool,
    /// 旋转 180°
    #[arg(long = "r180")]
    pub r180: bool,
    /// 水平镜像
    #[arg(long = "flip-h")]
    pub flip_h: bool,
    /// 垂直镜像
    #[arg(long = "flip-v")]
    pub flip_v: bool,
    /// 重置方向为正常（清除旋转标记）
    #[arg(long)]
    pub reset: bool,
}

// ----------------------------- copy -----------------------------

#[derive(Args, Debug)]
pub struct CopyArgs {
    /// 参考照片：从它读取元数据
    #[arg(long, value_name = "参考文件")]
    pub from: PathBuf,

    #[command(flatten)]
    pub target: TargetArgs,
    #[command(flatten)]
    pub write: WriteArgs,

    /// 复制拍摄时间相关字段
    #[arg(long)]
    pub time: bool,
    /// 复制 GPS 定位
    #[arg(long)]
    pub gps: bool,
    /// 复制全部可复制的元数据（默认；会跳过尺寸/方向等与具体图像绑定的字段）
    #[arg(long)]
    pub all: bool,
}

// ---------------------------- rename ----------------------------

#[derive(Args, Debug)]
pub struct RenameArgs {
    #[command(flatten)]
    pub target: TargetArgs,

    /// 文件名模板（strftime 语法，不含扩展名），默认 %Y%m%d_%H%M%S
    #[arg(long, value_name = "模板", default_value = "%Y%m%d_%H%M%S")]
    pub pattern: String,

    /// 仅预览，不实际重命名
    #[arg(short = 'n', long)]
    pub dry_run: bool,

    /// 跳过确认提示
    #[arg(short = 'y', long)]
    pub yes: bool,

    /// 详细输出
    #[arg(short, long)]
    pub verbose: bool,
}

// ----------------------------- xmp ------------------------------

#[derive(Args, Debug)]
pub struct XmpArgs {
    #[command(flatten)]
    pub target: TargetArgs,
    #[command(flatten)]
    pub write: WriteArgs,

    /// 标题（dc:title）
    #[arg(long, value_name = "文本")]
    pub title: Option<String>,
    /// 描述 / 说明（dc:description）
    #[arg(long, value_name = "文本")]
    pub description: Option<String>,
    /// 作者（dc:creator，可重复）
    #[arg(long, value_name = "文本")]
    pub creator: Vec<String>,
    /// 版权（dc:rights）
    #[arg(long, value_name = "文本")]
    pub rights: Option<String>,
    /// 评分 0-5（xmp:Rating）
    #[arg(long, value_name = "0-5")]
    pub rating: Option<i32>,
    /// 颜色标签（xmp:Label）
    #[arg(long, value_name = "文本")]
    pub label: Option<String>,
    /// 关键词，逗号分隔（dc:subject）
    #[arg(long, value_name = "a,b,c")]
    pub keywords: Option<String>,
    /// 城市（photoshop:City）
    #[arg(long, value_name = "文本")]
    pub city: Option<String>,
    /// 国家（photoshop:Country）
    #[arg(long, value_name = "文本")]
    pub country: Option<String>,

    /// 通用设置任意 XMP 属性，格式 前缀:名称=值（可重复）
    #[arg(long = "set", value_name = "qname=值")]
    pub set: Vec<String>,

    /// 删除指定 XMP 属性，按限定名如 dc:title（可重复）
    #[arg(long = "remove", value_name = "qname")]
    pub remove: Vec<String>,

    /// 清除整个 XMP 包
    #[arg(long)]
    pub clear: bool,
}

// ----------------------------- iptc -----------------------------

#[derive(Args, Debug)]
pub struct IptcArgs {
    #[command(flatten)]
    pub target: TargetArgs,
    #[command(flatten)]
    pub write: WriteArgs,

    /// 标题（2:05 Object Name）
    #[arg(long, value_name = "文本")]
    pub title: Option<String>,
    /// 说明 / 图注（2:120 Caption）
    #[arg(long, value_name = "文本")]
    pub description: Option<String>,
    /// 关键词，逗号分隔（2:25，可重复）
    #[arg(long, value_name = "a,b,c")]
    pub keywords: Option<String>,
    /// 作者 / 摄影师（2:80 By-line，可重复）
    #[arg(long, value_name = "文本")]
    pub creator: Vec<String>,
    /// 标题行（2:105 Headline）
    #[arg(long, value_name = "文本")]
    pub headline: Option<String>,
    /// 城市（2:90）
    #[arg(long, value_name = "文本")]
    pub city: Option<String>,
    /// 省 / 州（2:95）
    #[arg(long, value_name = "文本")]
    pub state: Option<String>,
    /// 国家（2:101）
    #[arg(long, value_name = "文本")]
    pub country: Option<String>,
    /// 版权（2:116）
    #[arg(long, value_name = "文本")]
    pub copyright: Option<String>,
    /// 提供者（2:110 Credit）
    #[arg(long, value_name = "文本")]
    pub credit: Option<String>,
    /// 来源（2:115 Source）
    #[arg(long, value_name = "文本")]
    pub source: Option<String>,
    /// 特殊说明（2:40 Instructions）
    #[arg(long, value_name = "文本")]
    pub instructions: Option<String>,

    /// 通用设置任意数据集，格式 名称=值 或 记录:数据集=值（可重复）
    #[arg(long = "set", value_name = "字段=值")]
    pub set: Vec<String>,

    /// 删除指定数据集，按名称或 记录:数据集（可重复）
    #[arg(long = "remove", value_name = "字段")]
    pub remove: Vec<String>,

    /// 清除整个 IPTC 块
    #[arg(long)]
    pub clear: bool,
}
