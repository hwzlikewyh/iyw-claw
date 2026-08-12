import struct
import zlib
from pathlib import Path
from xml.sax.saxutils import escape
from zipfile import ZipFile


def png_bytes() -> bytes:
    def chunk(kind: bytes, data: bytes) -> bytes:
        checksum = zlib.crc32(kind + data) & 0xFFFFFFFF
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", checksum)

    header = struct.pack(">IIBBBBB", 1, 1, 8, 6, 0, 0, 0)
    pixels = zlib.compress(b"\x00\xff\xff\xff\xff")
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", pixels)
        + chunk(b"IEND", b"")
    )


def write_test_png(path: Path) -> None:
    path.write_bytes(png_bytes())


def write_minimal_pptx(path: Path) -> None:
    content_types = '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml" /></Types>'
    presentation = '<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:sldSz cx="12192000" cy="6858000" /></p:presentation>'
    with ZipFile(path, "w") as archive:
        archive.writestr("[Content_Types].xml", content_types)
        archive.writestr("ppt/presentation.xml", presentation)


def sales_ppt_manifest(company_key: str) -> dict[str, object]:
    return {
        "company_key": company_key,
        "sections": [
            "business_info",
            "product_images",
            "market_keywords",
            "store_links",
            "activities",
            "materials",
            "opening_copy",
        ],
        "qa": {
            "status": "completed",
            "rendered_slides": 6,
            "overlap_checked": True,
            "text_fit_checked": True,
        },
    }


def write_prepared_sales_pptx(path: Path, company_name: str) -> None:
    slide_overrides = "".join(
        f'<Override PartName="/ppt/slides/slide{index}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml" />'
        for index in range(1, 7)
    )
    content_types = f'<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml" />{slide_overrides}</Types>'
    presentation = '<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:sldSz cx="12192000" cy="6858000" /></p:presentation>'
    with ZipFile(path, "w") as archive:
        archive.writestr("[Content_Types].xml", content_types)
        archive.writestr("ppt/presentation.xml", presentation)
        for index in range(1, 7):
            text = escape(f"{company_name} 销售资料 第{index}页")
            slide = f'<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>{text}</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>'
            archive.writestr(f"ppt/slides/slide{index}.xml", slide)
