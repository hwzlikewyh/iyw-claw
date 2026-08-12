from __future__ import annotations

from pathlib import Path
from typing import Any
from xml.etree import ElementTree
from zipfile import BadZipFile, ZipFile

CONTENT_TYPES = "[Content_Types].xml"
PRESENTATION = "ppt/presentation.xml"
MIN_SALES_SLIDES = 6
REQUIRED_SECTIONS = {
    "business_info",
    "product_images",
    "market_keywords",
    "store_links",
    "activities",
    "materials",
    "opening_copy",
}


def _local_name(tag: object) -> str:
    return str(tag).rsplit("}", 1)[-1]


def _valid_roots(
    content_types: ElementTree.Element, presentation: ElementTree.Element
) -> bool:
    if (
        _local_name(content_types.tag) != "Types"
        or _local_name(presentation.tag) != "presentation"
    ):
        return False
    return str(content_types.tag).startswith("{") and str(presentation.tag).startswith(
        "{"
    )


def _has_required_parts(
    content_types: ElementTree.Element, presentation: ElementTree.Element
) -> bool:
    overrides = {
        str(node.attrib.get("PartName") or "")
        for node in content_types
        if _local_name(node.tag) == "Override"
    }
    return "/ppt/presentation.xml" in overrides and any(
        _local_name(node.tag) == "sldSz" for node in presentation
    )


def _company_key(record: dict[str, Any]) -> str:
    company = record.get("company")
    if not isinstance(company, dict):
        return ""
    return str(company.get("lixiao_id") or company.get("name") or "").strip()


def _valid_manifest(record: dict[str, Any]) -> dict[str, Any] | None:
    outreach = record.get("outreach")
    manifest = outreach.get("ppt_manifest") if isinstance(outreach, dict) else None
    if not isinstance(manifest, dict) or manifest.get("company_key") != _company_key(
        record
    ):
        return None
    sections = manifest.get("sections")
    qa = manifest.get("qa")
    if not isinstance(sections, list) or not REQUIRED_SECTIONS.issubset(set(sections)):
        return None
    if not isinstance(qa, dict) or qa.get("status") != "completed":
        return None
    if qa.get("overlap_checked") is not True or qa.get("text_fit_checked") is not True:
        return None
    rendered = qa.get("rendered_slides")
    return (
        manifest if isinstance(rendered, int) and rendered >= MIN_SALES_SLIDES else None
    )


def _slide_names(archive: ZipFile) -> list[str]:
    return sorted(
        name
        for name in archive.namelist()
        if name.startswith("ppt/slides/slide") and name.endswith(".xml")
    )


def _slides_contain_company(
    archive: ZipFile, slide_names: list[str], company_name: str
) -> bool:
    text: list[str] = []
    for name in slide_names:
        root = ElementTree.fromstring(archive.read(name))
        text.extend(
            str(node.text or "") for node in root.iter() if _local_name(node.tag) == "t"
        )
    normalized = "".join("".join(text).split()).casefold()
    expected = "".join(company_name.split()).casefold()
    return bool(expected and expected in normalized)


def prepared_presentation_path(record: dict[str, Any]) -> Path | None:
    outreach = record.get("outreach")
    manifest = _valid_manifest(record)
    value = outreach.get("ppt_path") if isinstance(outreach, dict) else None
    path = Path(str(value or ""))
    if manifest is None or path.suffix.casefold() != ".pptx" or not path.is_file():
        return None
    try:
        with ZipFile(path) as archive:
            names = set(archive.namelist())
            if not {CONTENT_TYPES, PRESENTATION}.issubset(names):
                return None
            content_types = ElementTree.fromstring(archive.read(CONTENT_TYPES))
            presentation = ElementTree.fromstring(archive.read(PRESENTATION))
            slides = _slide_names(archive)
            company = record.get("company") or {}
            company_name = str(company.get("name") or "")
            rendered = int(manifest["qa"]["rendered_slides"])
            content_valid = (
                len(slides) >= MIN_SALES_SLIDES
                and len(slides) == rendered
                and _slides_contain_company(archive, slides, company_name)
            )
    except (ElementTree.ParseError, BadZipFile, OSError):
        return None
    if not _valid_roots(content_types, presentation) or not content_valid:
        return None
    return path if _has_required_parts(content_types, presentation) else None
