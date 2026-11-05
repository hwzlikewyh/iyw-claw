use iyw_share_core::{BatchWriter, Inbox, MAX_FILE_BYTES};
use std::{fs, io::Write, path::PathBuf};
use windows::{
    core::Interface,
    ApplicationModel::Activation::ShareTargetActivatedEventArgs,
    ApplicationModel::DataTransfer::{ShareTarget::ShareOperation, StandardDataFormats},
    ApplicationModel::{AppInfo, AppInstance, Package},
    Storage::{NameCollisionOption, StorageFile, StorageFolder},
    Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

struct Runtime;
impl Drop for Runtime {
    fn drop(&mut self) {
        unsafe { RoUninitialize() };
    }
}

pub fn run() -> Result<()> {
    unsafe {
        RoInitialize(RO_INIT_MULTITHREADED)?;
    }
    let _runtime = Runtime;
    if std::env::args().any(|arg| arg == "--registration-check") {
        return check_identity();
    }
    let activation = AppInstance::GetActivatedEventArgs()?;
    let arguments: ShareTargetActivatedEventArgs = activation.cast()?;
    let operation = arguments.ShareOperation()?;
    operation.ReportStarted()?;
    match receive(&operation) {
        Ok(id) => {
            // 报告完成前批次已落盘。主应用唤醒失败不会让文件丢失。
            log(&format!("share committed: batch={id}"));
            if let Err(error) = launch_main() {
                log(&format!("main launch failed: {error}"));
            }
            operation.ReportCompleted()?;
            Ok(())
        }
        Err(error) => {
            let _ = operation.ReportError(&"无法接收分享文件，请在原助理中重试导入。".into());
            Err(error)
        }
    }
}

fn check_identity() -> Result<()> {
    let package = Package::Current()?.Id()?;
    let expected_app = format!("{}!Share", package.FamilyName()?);
    if package.Name()?.to_string() != "IywAssistant.ShareTarget" {
        return Err("分享接收程序取得了其它包的身份".into());
    }
    if package.Publisher()?.to_string() != env!("IYW_SHARE_PUBLISHER") {
        return Err("分享包的发布者与接收程序不一致".into());
    }
    if AppInfo::Current()?.AppUserModelId()?.to_string() != expected_app {
        return Err("分享接收程序的应用标识不匹配".into());
    }
    Ok(())
}

fn receive(operation: &ShareOperation) -> Result<String> {
    let data = operation.Data()?;
    if !data.Contains(&StandardDataFormats::StorageItems()?)? {
        return Err("此入口需要微信导出的聊天 ZIP 或文件附件".into());
    }
    let items = data.GetStorageItemsAsync()?.get()?;
    let count = items.Size()?;
    if count == 0 || count as usize > iyw_share_core::MAX_FILES {
        return Err("分享文件数量无效".into());
    }
    let mut writer = Inbox::new(iyw_share_core::default_root()?).begin()?;
    for index in 0..count {
        let file: StorageFile = items.GetAt(index)?.cast()?;
        copy_shared_file(&mut writer, &file)?;
    }
    Ok(writer.commit()?.id)
}

fn copy_shared_file(writer: &mut BatchWriter, source: &StorageFile) -> Result<()> {
    let size = source.GetBasicPropertiesAsync()?.get()?.Size()?;
    if size == 0 || size > MAX_FILE_BYTES {
        return Err("分享文件为空或超过 512 MiB".into());
    }
    let name = source.Name()?.to_string();
    let destination = writer.destination(&name)?;
    let parent = destination.parent().ok_or("分享目录不存在")?;
    let folder =
        StorageFolder::GetFolderFromPathAsync(&parent.to_string_lossy().as_ref().into())?.get()?;
    let filename = destination
        .file_name()
        .ok_or("分享文件名无效")?
        .to_string_lossy();
    source
        .CopyOverload(
            &folder,
            &filename.as_ref().into(),
            NameCollisionOption::FailIfExists,
        )?
        .get()?;
    writer.record_file(&destination, &name)?;
    Ok(())
}

fn launch_main() -> Result<()> {
    let executable = std::env::current_exe()?;
    let root = executable
        .parent()
        .and_then(|path| path.parent())
        .ok_or("应用目录不存在")?;
    let main = root.join("iyw-claw.exe");
    if !main.is_file() {
        return Err("原助理主程序不存在，文件已保留".into());
    }
    use std::os::windows::process::CommandExt;
    std::process::Command::new(main)
        .arg("--from-native-share")
        .creation_flags(0x08000000)
        .spawn()?;
    Ok(())
}

pub fn log(message: &str) {
    let Ok(root) = iyw_share_core::default_root() else {
        return;
    };
    let _ = fs::create_dir_all(&root);
    let path: PathBuf = root.join("reception.log");
    // 只记录编号和错误，不记录分享正文或文件名。
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{message}");
    }
}
