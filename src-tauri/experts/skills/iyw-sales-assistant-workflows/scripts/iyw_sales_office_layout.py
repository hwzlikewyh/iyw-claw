from __future__ import annotations

from dataclasses import dataclass


FONT = "Microsoft YaHei"
ACCENT = "1F4E78"
HEADER = "D9EAF7"
COLUMNS = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"


@dataclass(frozen=True)
class SheetSpec:
    name: str
    title: str
    headers: tuple[str, ...]
    rows: tuple[tuple[object, ...], ...]
    widths: tuple[int, ...]
    empty_text: str = "暂无数据"


def display(value: object) -> str:
    if value is None or value == "":
        return "未提供"
    if isinstance(value, (list, tuple)):
        return "、".join(display(item) for item in value) or "未提供"
    return str(value)


def _cell(path: str, value: object, **props: object) -> dict[str, object]:
    return {
        "command": "set",
        "path": path,
        "props": {"value": display(value), "font.name": FONT, **props},
    }


def _body_commands(spec: SheetSpec, end_col: str) -> list[dict[str, object]]:
    commands: list[dict[str, object]] = []
    if not spec.rows:
        return [
            _cell(
                f"/{spec.name}/A4",
                spec.empty_text,
                merge=f"A4:{end_col}4",
                **{"font.color": "666666", "font.italic": True},
            )
        ]
    for row_index, row in enumerate(spec.rows, 4):
        for column_index, value in enumerate(row):
            commands.append(
                _cell(
                    f"/{spec.name}/{COLUMNS[column_index]}{row_index}",
                    value,
                    **{"alignment.wrapText": True, "alignment.vertical": "top"},
                )
            )
    return commands


def sheet_commands(spec: SheetSpec) -> list[dict[str, object]]:
    end_col = COLUMNS[len(spec.headers) - 1]
    commands: list[dict[str, object]] = [
        {
            "command": "set",
            "path": f"/{spec.name}",
            "props": {
                "freeze": "A4",
                "autoFilter": f"A3:{end_col}{max(4, len(spec.rows) + 3)}",
                "orientation": "landscape",
                "fitToPage": "1x0",
            },
        },
        _cell(
            f"/{spec.name}/A1",
            spec.title,
            merge=f"A1:{end_col}1",
            fill=ACCENT,
            **{"font.color": "FFFFFF", "font.bold": True, "font.size": "18pt"},
        ),
        {"command": "set", "path": f"/{spec.name}/row[1]", "props": {"height": 28}},
    ]
    for index, header in enumerate(spec.headers):
        commands.append(
            _cell(
                f"/{spec.name}/{COLUMNS[index]}3",
                header,
                fill=HEADER,
                **{"font.bold": True, "alignment.wrapText": True},
            )
        )
    commands.extend(_body_commands(spec, end_col))
    for index, width in enumerate(spec.widths):
        commands.append(
            {
                "command": "set",
                "path": f"/{spec.name}/col[{COLUMNS[index]}]",
                "props": {"width": width},
            }
        )
    return commands
