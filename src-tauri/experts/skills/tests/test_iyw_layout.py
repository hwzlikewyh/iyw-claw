import asyncio
import sys
from pathlib import Path

import pytest
from PIL import Image


SCRIPTS_DIR = Path(__file__).parents[1] / "iyw-image-workflows" / "scripts"
sys.path.insert(0, str(SCRIPTS_DIR))

import iyw_commerce  # noqa: E402
from iyw_image import IywError  # noqa: E402
from iyw_layout import compose_layout  # noqa: E402


COLORS = [
    (255, 0, 0),
    (0, 255, 0),
    (0, 0, 255),
    (255, 255, 0),
]


def _image(path: Path, color: tuple[int, int, int], size=(20, 20)) -> Path:
    Image.new("RGB", size, color).save(path)
    return path


def _fixtures(tmp_path: Path, sizes=None) -> list[Path]:
    actual_sizes = sizes or [(20, 20)] * 4
    return [
        _image(tmp_path / f"{index}.png", color, actual_sizes[index])
        for index, color in enumerate(COLORS)
    ]


def test_compose_two_by_two_preserves_order_ratio_and_gap(tmp_path):
    images = _fixtures(tmp_path, [(20, 10), (10, 20), (20, 20), (5, 5)])
    out = tmp_path / "layout.png"

    result = compose_layout(images, 2, 2, out, gap=2, background="#FFFFFF")

    assert result == {
        "out": str(out.resolve()),
        "width": 42,
        "height": 42,
        "rows": 2,
        "columns": 2,
        "count": 4,
    }
    with Image.open(out) as composed:
        assert composed.size == (42, 42)
        assert composed.getpixel((10, 10)) == COLORS[0]
        assert composed.getpixel((32, 10)) == COLORS[1]
        assert composed.getpixel((10, 32)) == COLORS[2]
        assert composed.getpixel((32, 32)) == COLORS[3]
        assert composed.getpixel((10, 1)) == (255, 255, 255)
        assert composed.getpixel((21, 21)) == (255, 255, 255)


def test_compose_one_by_four_keeps_input_order(tmp_path):
    images = _fixtures(tmp_path)
    out = tmp_path / "strip.webp"

    compose_layout(images, 1, 4, out)

    with Image.open(out) as composed:
        assert composed.size == (80, 20)
        for index, color in enumerate(COLORS):
            actual = composed.getpixel((index * 20 + 10, 10))
            assert all(abs(actual[channel] - color[channel]) <= 2 for channel in range(3))


@pytest.mark.parametrize(
    ("rows", "columns", "gap", "background", "suffix", "message"),
    [
        (0, 2, 0, "#FFFFFF", ".png", "positive"),
        (2, 0, 0, "#FFFFFF", ".png", "positive"),
        (2, 2, -1, "#FFFFFF", ".png", "nonnegative"),
        (2, 2, 0, "white", ".png", "#RRGGBB"),
        (2, 2, 0, "#FFFFFF", ".bmp", "output extension"),
    ],
)
def test_compose_rejects_invalid_layout_options(
    tmp_path, rows, columns, gap, background, suffix, message
):
    images = _fixtures(tmp_path)

    with pytest.raises(IywError, match=message):
        compose_layout(
            images,
            rows,
            columns,
            tmp_path / f"layout{suffix}",
            gap=gap,
            background=background,
        )


def test_compose_requires_exact_image_count(tmp_path):
    images = _fixtures(tmp_path)[:3]

    with pytest.raises(IywError, match="exactly 4"):
        compose_layout(images, 2, 2, tmp_path / "layout.png")


def test_compose_rejects_missing_input_and_output_parent(tmp_path):
    images = _fixtures(tmp_path)
    images[0] = tmp_path / "missing.png"

    with pytest.raises(IywError, match="not found"):
        compose_layout(images, 2, 2, tmp_path / "layout.png")

    images = _fixtures(tmp_path)
    with pytest.raises(IywError, match="output directory"):
        compose_layout(images, 2, 2, tmp_path / "missing" / "layout.png")


def test_compose_rejects_unsupported_input_extension(tmp_path):
    images = _fixtures(tmp_path)
    unsupported = tmp_path / "input.bmp"
    Image.new("RGB", (20, 20), COLORS[0]).save(unsupported)
    images[0] = unsupported

    with pytest.raises(IywError, match="unsupported input image extension"):
        compose_layout(images, 2, 2, tmp_path / "layout.png")


def test_compose_rejects_corrupt_input_image(tmp_path):
    images = _fixtures(tmp_path)
    corrupt = tmp_path / "corrupt.png"
    corrupt.write_bytes(b"not an image")
    images[0] = corrupt

    with pytest.raises(IywError, match="could not decode input image"):
        compose_layout(images, 2, 2, tmp_path / "layout.png")


def test_compose_closes_canvas_when_save_fails(tmp_path, monkeypatch):
    import iyw_layout

    images = _fixtures(tmp_path)
    canvas = Image.new("RGB", (40, 40))
    closed = False
    original_close = canvas.close

    def close_canvas():
        nonlocal closed
        closed = True
        original_close()

    monkeypatch.setattr(canvas, "close", close_canvas)
    monkeypatch.setattr(iyw_layout, "_compose_canvas", lambda *_args: canvas)
    monkeypatch.setattr(
        iyw_layout,
        "_save_atomic",
        lambda *_args: (_ for _ in ()).throw(IywError("save failed", "invalid_input")),
    )

    with pytest.raises(IywError, match="save failed"):
        compose_layout(images, 2, 2, tmp_path / "layout.png")

    assert closed is True


def test_compose_cleans_temporary_file_when_replace_fails(tmp_path, monkeypatch):
    images = _fixtures(tmp_path)
    out = tmp_path / "layout.png"

    def fail_replace(_self, _target):
        raise RuntimeError("replace failed")

    monkeypatch.setattr(Path, "replace", fail_replace)

    with pytest.raises(IywError, match="could not write output image"):
        compose_layout(images, 2, 2, out)

    assert not out.exists()
    assert not list(tmp_path.glob(".layout.*.png"))


def test_compose_rejects_existing_output_unless_forced(tmp_path):
    images = _fixtures(tmp_path)
    out = _image(tmp_path / "layout.png", (1, 2, 3))

    with pytest.raises(IywError, match="already exists"):
        compose_layout(images, 2, 2, out)

    result = compose_layout(images, 2, 2, out, force=True)

    assert result["out"] == str(out.resolve())
    with Image.open(out) as composed:
        assert composed.size == (40, 40)


def test_compose_cli_does_not_initialize_api_client(tmp_path, monkeypatch):
    images = _fixtures(tmp_path)
    out = tmp_path / "layout.jpg"
    monkeypatch.setattr(
        iyw_commerce,
        "_client",
        lambda *_args, **_kwargs: pytest.fail("API client must not be initialized"),
    )
    args = iyw_commerce.build_parser().parse_args(
        [
            "compose-layout",
            *[item for image in images for item in ("--image", str(image))],
            "--rows",
            "2",
            "--columns",
            "2",
            "--out",
            str(out),
        ]
    )

    result = asyncio.run(iyw_commerce.run_command(args))

    assert result["count"] == 4
    with Image.open(out) as composed:
        assert composed.size == (40, 40)


def test_compose_cli_main_succeeds_without_connection_args(
    tmp_path, monkeypatch, capsys
):
    images = _fixtures(tmp_path)
    out = tmp_path / "layout.png"
    argv = [
        "iyw_commerce.py",
        "compose-layout",
        *[item for image in images for item in ("--image", str(image))],
        "--rows",
        "2",
        "--columns",
        "2",
        "--out",
        str(out),
    ]
    monkeypatch.setattr(sys, "argv", argv)
    monkeypatch.setattr(
        iyw_commerce,
        "_client",
        lambda *_args, **_kwargs: pytest.fail("API client must not be initialized"),
    )

    exit_code = iyw_commerce.main()

    assert exit_code == 0
    assert '"ok": true' in capsys.readouterr().out
    with Image.open(out) as composed:
        assert composed.size == (40, 40)
