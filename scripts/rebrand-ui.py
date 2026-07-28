"""Replace visible 'iyw-claw' brand strings with '原助理'.

Only user-facing display strings are touched. Internal identifiers
(iyw-claw:// URI scheme, iyw-claw: storage keys, iyw-claw. config keys,
CSS class names, localStorage keys, keychain descriptions, file extensions,
WebSocket protocols, binary names) are left intact.
"""
import io

BRAND = "原助理"

# (path, [(exact_old, exact_new), ...])
PATCHES = [
    # window titles built in JS/TS (acp-connections-context.tsx)
    ("src/contexts/acp-connections-context.tsx", [
        ("- iyw-claw`", f"- {BRAND}`"),
        (': "iyw-claw"', f': "{BRAND}"'),
    ]),
    # boot-loading screen text
    ("src/components/layout/app-boot-loading.tsx", [
        (">iyw-claw<", f">{BRAND}<"),
    ]),
    # exported HTML / Markdown footer
    ("src/lib/export-conversation.ts", [
        (">iyw-claw<", f">{BRAND}<"),
        ("*iyw-claw*", f"*{BRAND}*"),
    ]),
    # settings window title
    ("src/components/settings/settings-shell.tsx", [
        ("- iyw-claw`", f"- {BRAND}`"),
    ]),
    # git sub-page titles
    ("src/app/commit/page.tsx",  [("- iyw-claw`", f"- {BRAND}`")]),
    ("src/app/merge/page.tsx",   [("- iyw-claw`", f"- {BRAND}`")]),
    ("src/app/push/page.tsx",    [("- iyw-claw`", f"- {BRAND}`")]),
    ("src/app/stash/page.tsx",   [("- iyw-claw`", f"- {BRAND}`")]),
    # workspace layout title
    ("src/app/workspace/layout.tsx", [
        ("- iyw-claw`", f"- {BRAND}`"),
        (': "iyw-claw"', f': "{BRAND}"'),
    ]),
    # pet window title
    ("src/app/pet/_components/PetWindow.tsx", [
        ('iyw-claw pet"', f'{BRAND} 宠物"'),
    ]),
    # channel-events sample body text (user-visible documentation example)
    ("src/components/settings/channel-events-tab.tsx", [
        ("Answer it in iyw-claw.", f"Answer it in {BRAND}."),
    ]),
    # backup dialog display names (keep extension 'iyw-clawbak' unchanged)
    ("src/lib/api.ts", [
        ('"iyw-claw backup"',   f'"{BRAND} 备份"'),
        ("iyw-claw-backup-",    f"{BRAND}-backup-"),
    ]),
    ("src/components/settings/backup-settings.tsx", [
        ('{ name: "iyw-claw backup"', f'{{ name: "{BRAND} 备份"'),
    ]),
]

modified = []
for path, subs in PATCHES:
    try:
        text = io.open(path, encoding="utf-8").read()
    except FileNotFoundError:
        print(f"SKIP (not found): {path}")
        continue
    original = text
    for old, new in subs:
        n = text.count(old)
        text = text.replace(old, new)
        if n:
            print(f"  [{n}x] {path!r}: {old!r} -> {new!r}")
    if text != original:
        io.open(path, "w", encoding="utf-8", newline="\n").write(text)
        modified.append(path)

print(f"\nDone. Modified {len(modified)} files.")
