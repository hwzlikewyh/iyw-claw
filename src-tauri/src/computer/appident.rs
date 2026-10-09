//! Which application a process is, asked of the process itself.
//!
//! The driver has its own answer — `list_apps` — and on macOS it goes stale.
//! It reads `NSWorkspace.runningApplications`, which only changes while the
//! main run loop runs, and the driver never runs its: an application launched
//! after the driver started is missing from that list for good, and one that
//! quit stays on it — under a pid the system may since have handed to
//! another process, which would then pass for it. So on macOS the helper asks
//! the kernel which executable a process runs (`proc_pidpath`, an ordinary
//! BSD query, not TCC-governed) and reads the application off that.
//!
//! Only the main executable of an application bundle counts
//! (`<bundle>/Contents/MacOS/<exe>`), where the bundle is an application:
//! named `<name>.app`, or saying so itself (`CFBundlePackageType` `APPL`).
//! The second is how Chromium browsers run — from a clone of their bundle,
//! `<name>.app.bundle` in a temporary folder, made at launch so that an update
//! replacing the installed copy leaves the running code its signature. Such a
//! clone is on a blocklist by its bundle identifier or by the file name it was
//! cloned from, not by the installed copy's full path, which it does not
//! carry. An XPC service, an app extension or a bare executable draws windows
//! for another application or for nobody in particular — the password
//! AutoFill panel is one — and stays unidentified, which is what the driver's
//! list of regular applications left them as. So do Apple's own agents under
//! `/System` — the login window, the Gatekeeper and keychain prompts, Control
//! Center — except in the places Apple keeps the applications people use.
//!
//! A helper application inside another (`Foo.app/…/Foo Helper.app`) is the
//! application it sits in: its windows are that application's, and so is its
//! place on a blocklist — the Passwords menu-bar helper is Passwords. Only one
//! named `.app`, though: a bundle that is an application by its own word
//! alone is one only standing by itself, since inside another it would pass
//! for that one — a clone of a blocklisted application put inside an
//! application nobody listed would take that one's name. And an
//! application is known by its bundle identifier: one whose `Info.plist`
//! cannot be read is not told apart by its path instead, since the blocklist
//! names password managers by identifier.
//!
//! **Names.** An application is called what the Finder calls it: its file
//! name — Visual Studio Code, which calls itself `Code` — unless it calls
//! itself something else in the person's language: the Finder is 访达 in
//! Chinese, and WPS Office's file is `wpsoffice.app`. That name is the one
//! the window list carries for each window's owner, so a name there that the
//! bundle's own (untranslated) `Info.plist` does not have is taken as a
//! translation. The Finder's own translated name is not to be had here: a
//! process with no bundle of its own is answered in the development language.
//!
//! **Windows.** There the driver's answer names no application at all: its
//! list carries each process's executable by file name alone, and a path only
//! for the few whose file name happens to match a Start menu shortcut — none,
//! on many machines. So the helper reads the full path off the process itself
//! (with its start time, through the same handle: see `procinfo`), and the
//! executable is the application — unless it draws for other applications,
//! or is the system's own. `ApplicationFrameHost.exe` draws the frame of every
//! packaged application's window — Settings, Calculator — while the
//! application draws what is in it, in a window of its own process set inside
//! the frame; `msedgewebview2.exe` draws the inspector and the dialogs of
//! every application built on WebView2, iyw-claw among them. A window of either
//! is any of those applications', and which one cannot be told from the
//! host's path: taken for the host, it would pass for an application no
//! blocklist names, and for one that is not iyw-claw. So a frame is taken for
//! the application of the process drawing inside it, which the helper finds
//! by the frame's handle (see `helper::hwnd`) — and for as long as that same
//! run of it is inside; the frame host lends its windows nothing of its own.
//! A window of WebView2's stays unidentified, as on macOS does a process
//! drawing for another application. And the system's own agents stay
//! unidentified, as Apple's under `/System` do, framed or not: the Start
//! menu, the lock screen, the prompts for a PIN or for an account's password,
//! which Windows keeps each in a folder of its own in `SystemApps`.
//!
//! On Windows an application is called what Windows calls it to the person.
//! A packaged one — Terminal, Settings, the Notepad Windows 11 ships — goes by
//! its entry in the Start menu (the shell's Apps folder), in the person's
//! language: 终端, 设置, 记事本 in Chinese. Its executable's own description
//! is no name for it: Terminal's says "Windows Terminal Host", Notepad's
//! "Notepad.exe", Photos' nothing at all. Any other application goes by what
//! Task Manager calls it: the description in its executable's version
//! resource, in the person's language where Windows carries a translation —
//! Explorer is "Windows 资源管理器" in Chinese — or, where it gives none, its
//! file name. A name only says what to call an application, never what it is
//! (its path does), so each is read once per executable and kept: the Start
//! menu can take a fifth of a second to answer.

#[path = "parts/appident/HOSTS_group_1.rs"]
mod part_1;
use part_1::*;

#[cfg(windows)]
#[path = "parts/appident/MAX_WINDOWS_NAMES_group_2.rs"]
mod part_2;
#[cfg(windows)]
use part_2::*;

#[path = "parts/appident/SYSTEM_APPLICATIONS_group_3.rs"]
mod part_3;
use part_3::*;

#[cfg(target_os = "macos")]
#[path = "parts/appident/MAX_INFO_PLIST_group_4.rs"]
mod part_4;
#[cfg(target_os = "macos")]
use part_4::*;

#[path = "parts/appident/AppIdentity_group_5.rs"]
mod part_5;
pub use part_5::*;

#[path = "parts/appident/name_group_6.rs"]
mod part_6;

#[path = "parts/appident/file_name_group_7.rs"]
mod part_7;
use part_7::*;

#[path = "parts/appident/has_app_extension_group_8.rs"]
mod part_8;
pub use part_8::*;

#[path = "parts/appident/is_application_outside_group_9.rs"]
mod part_9;
use part_9::*;

#[cfg(windows)]
#[path = "parts/appident/system_apps_folder_group_10.rs"]
mod part_10;
#[cfg(windows)]
use part_10::*;

#[cfg(not(windows))]
#[path = "parts/appident/system_apps_folder_group_11.rs"]
mod part_11;
#[cfg(not(windows))]
use part_11::*;

#[path = "parts/appident/is_frame_host_group_12.rs"]
mod part_12;
pub use part_12::*;

#[cfg(target_os = "macos")]
#[path = "parts/appident/identify_group_13.rs"]
mod part_13;
#[cfg(target_os = "macos")]
pub use part_13::*;

#[cfg(windows)]
#[path = "parts/appident/WindowsApp_group_14.rs"]
mod part_14;
#[cfg(windows)]
pub use part_14::*;

#[cfg(windows)]
#[path = "parts/appident/windows_name_group_15.rs"]
mod part_15;
#[cfg(windows)]
use part_15::*;

#[cfg(windows)]
pub(crate) use windows_names::Com;

/// Windows: the two places an application's name is read from (see the
/// module note).
#[cfg(windows)]
#[path = "parts/appident/windows_names.rs"]
mod windows_names;

#[cfg(target_os = "macos")]
use part_16::*;
