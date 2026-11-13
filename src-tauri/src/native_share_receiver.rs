use iyw_share_core::{BatchWriter, Inbox, MAX_FILE_BYTES};
use std::{fs, io::Write};
use windows::{
    core::Interface,
    ApplicationModel::Activation::{ActivationKind, ShareTargetActivatedEventArgs},
    ApplicationModel::DataTransfer::{ShareTarget::ShareOperation, StandardDataFormats},
    ApplicationModel::{AppInfo, AppInstance, Package},
    Storage::{NameCollisionOption, StorageFile, StorageFolder},
    Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const CAPABILITY: &str = "IYW_NATIVE_SHARE_MAIN_V1";

struct Runtime;
impl Drop for Runtime {
    fn drop(&mut self) {
        unsafe { RoUninitialize() };
    }
}

pub fn dispatch_early() -> bool {
    let arguments: Vec<_> = std::env::args_os().collect();
    if arguments
        .iter()
        .any(|arg| arg == "--native-share-capabilities")
    {
        println!("{CAPABILITY} {}", env!("CARGO_PKG_VERSION"));
        return true;
    }
    let checking = arguments
        .iter()
        .any(|arg| arg == "--native-share-registration-check");
    if !checking
        && arguments.iter().any(|arg| {
            matches!(
                arg.to_str(),
                Some("--credential-helper" | "--from-autostart" | "--from-native-share")
            )
        })
    {
        return false;
    }
    if let Err(error) = unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
        if checking {
            fail_check(error.into());
        }
        return false;
    }
    let _runtime = Runtime;
    if checking {
        if let Err(error) = check_identity() {
            fail_check(error);
        }
        return true;
    }
    receive_activation()
}

fn receive_activation() -> bool {
    let Ok(activation) = AppInstance::GetActivatedEventArgs() else {
        return false;
    };
    if activation.Kind().ok() != Some(ActivationKind::ShareTarget) {
        return false;
    }
    let result: Result<()> = activation
        .cast::<ShareTargetActivatedEventArgs>()
        .map_err(Into::into)
        .and_then(|arguments| accept(arguments.ShareOperation()?));
    if let Err(error) = result {
        log(&format!("share reception failed: {error}"));
    }
    true
}

fn fail_check(error: Box<dyn std::error::Error>) -> ! {
    eprintln!("{error}");
    std::process::exit(1);
}

fn check_identity() -> Result<()> {
    let identity = Package::Current()?.Id()?;
    let expected = format!("{}!Share.Hub", identity.FamilyName()?);
    let root = std::env::current_exe()?;
    let root = root.parent().ok_or("原助理安装目录不存在")?;
    let config: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("native-share/config.json"))?)?;
    if identity.Name()?.to_string() != "IywAssistant.Desktop"
        || AppInfo::Current()?.AppUserModelId()?.to_string() != expected
        || config["publisher"].as_str() != Some(identity.Publisher()?.to_string().as_str())
    {
        return Err("原助理主程序与系统分享包的身份不匹配".into());
    }
    Ok(())
}

fn accept(operation: ShareOperation) -> Result<()> {
    log("share activation received by main executable");
    operation.ReportStarted()?;
    match receive(&operation) {
        Ok(id) => {
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
    use std::os::windows::process::CommandExt;
    std::process::Command::new(std::env::current_exe()?)
        .arg("--from-native-share")
        .creation_flags(0x08000000)
        .spawn()?;
    Ok(())
}

fn log(message: &str) {
    let Ok(root) = iyw_share_core::default_root() else {
        return;
    };
    let _ = fs::create_dir_all(&root);
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("reception.log"))
    {
        let _ = writeln!(file, "pid={} {message}", std::process::id());
    }
}
