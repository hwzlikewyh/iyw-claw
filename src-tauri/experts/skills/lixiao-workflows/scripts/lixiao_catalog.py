from __future__ import annotations

import json
from dataclasses import dataclass


SEARCH_MODULES = json.dumps(
    [
        "searchFactory",
        "searchIndustryCustomer",
        "ecommercePlatform",
        "searchExhibitionJournal",
        "lastReg",
        "searchPatent",
        "searchExhibitionProvider",
        "searchExhibitor",
        "independentSiteSearch",
        "searchChannel",
        "searchProject",
        "customsDataSearch",
        "shop_search",
        "searchExhibitionHall",
        "searchTrademark",
    ],
    separators=(",", ":"),
)


@dataclass(frozen=True, kw_only=True)
class EndpointSpec:
    method: str
    service: str
    path: str
    auth: str
    description: str
    required_query: tuple[str, ...] = ()
    default_query: tuple[tuple[str, str], ...] = ()
    body_required: bool = False


def _spec(
    method: str,
    service: str,
    path: str,
    *,
    auth: str,
    description: str,
    required: tuple[str, ...] = (),
    defaults: tuple[tuple[str, str], ...] = (),
    body: bool = False,
) -> EndpointSpec:
    return EndpointSpec(
        method=method,
        service=service,
        path=path,
        auth=auth,
        description=description,
        required_query=required,
        default_query=defaults,
        body_required=body,
    )


BUSINESS_DEFAULTS = (
    ("industryColumnType", "industry_exhibition"),
    ("searchNewDataModule", SEARCH_MODULES),
)


SPECS: dict[str, EndpointSpec] = {
    "qr-start": _spec(
        "GET",
        "uc",
        "/api/auth/getCredence",
        auth="app",
        description="Create a QR login code",
    ),
    "qr-poll": _spec(
        "GET",
        "uc",
        "/api/auth/qrConnect",
        auth="app",
        description="Poll QR login state",
        required=("code",),
    ),
    "password-login": _spec(
        "POST",
        "uc",
        "/api/sso/login",
        auth="app",
        description="Log in with password and captcha proof",
        body=True,
    ),
    "captcha-register": _spec(
        "GET",
        "uc",
        "/api/geetest/register",
        auth="app",
        description="Create a Geetest challenge",
    ),
    "app-session": _spec(
        "GET",
        "uc",
        "/api/sso/getApp",
        auth="app",
        description="Get the application SSO session",
    ),
    "feature-packages": _spec(
        "GET",
        "skb",
        "/api_skb/v1/user/feature_packages",
        auth="business",
        description="List enabled feature packages",
    ),
    "scene-search": _spec(
        "POST",
        "skb",
        "/api_skb/v1/scene_search",
        auth="business",
        description="Run a scene search",
        body=True,
    ),
    "company-card": _spec(
        "GET",
        "enterprise",
        "/api_skb/v1/companyDetail/bizCard",
        auth="business",
        description="Get a company business card",
        required=("id",),
    ),
    "company-exhibitions": _spec(
        "GET",
        "enterprise",
        "/api_skb/v1/companyDetail/sectionInfo",
        auth="business",
        description="Get company exhibition data",
        required=("id",),
        defaults=(
            ("section", "ExhibitionJournalInfo"),
            ("pageSize", "10"),
            ("industryColumnType", "industry_exhibition"),
        ),
    ),
    "permission-info": _spec(
        "GET",
        "skb",
        "/api_skb/v1/user/permission_info",
        auth="business",
        description="Get company viewing permissions",
        defaults=(
            (
                "types",
                "crmImport,crmSetting,robotAuth,userPackageMap,recommender,enableAdvancedSearch",
            ),
        ),
    ),
    "phone-call-list": _spec(
        "POST",
        "skb",
        "/api_skb/v1/clue/dx_phone_call/list",
        auth="business",
        description="Get phone call records",
        body=True,
    ),
    "company-contacts": _spec(
        "GET",
        "skb",
        "/api_skb/v1/clue/contacts",
        auth="business",
        description="Get company contacts",
        required=("pid", "entName"),
        defaults=(("source", "scene_search.searchEcommercePlatformEnterprise_detail"),),
    ),
    "company-products": _spec(
        "GET",
        "enterprise",
        "/api_skb/v1/companyDetail/sectionInfo",
        auth="business",
        description="Get company products",
        required=("id",),
        defaults=(
            ("page", "1"),
            ("pageSize", "8"),
            ("label", "ShopGoodsInfo"),
            ("section", "Product"),
            *BUSINESS_DEFAULTS,
            ("sourceCode", ""),
        ),
    ),
    "company-base": _spec(
        "GET",
        "enterprise",
        "/api_skb/v1/companyDetail/sectionInfo",
        auth="business",
        description="Get company base information",
        required=("id",),
        defaults=(("section", "BaseInfo"),),
    ),
    "company-management": _spec(
        "GET",
        "enterprise",
        "/api_skb/v1/companyDetail/sectionInfo",
        auth="business",
        description="Get company management information",
        required=("id",),
        defaults=(("section", "ManageInfo"), *BUSINESS_DEFAULTS),
    ),
    "company-ip": _spec(
        "GET",
        "enterprise",
        "/api_skb/v1/companyDetail/sectionInfo",
        auth="business",
        description="Get company intellectual property",
        required=("id",),
        defaults=(("section", "IPR"),),
    ),
    "company-unlock": _spec(
        "GET",
        "skb",
        "/api_skb/v2/company/clue/chargers",
        auth="business",
        description="Unlock company viewing",
        required=("entityId",),
    ),
    "company-brand": _spec(
        "GET",
        "enterprise",
        "/api_skb/v1/companyDetail/sectionInfo",
        auth="business",
        description="Get company brand and outlet data",
        required=("id",),
        defaults=(("section", "Product"), *BUSINESS_DEFAULTS),
    ),
    "scene-search-products": _spec(
        "POST",
        "skb",
        "/api_skb/v1/scene_search",
        auth="business",
        description="Search ecommerce products",
        body=True,
    ),
    "company-recruitment": _spec(
        "GET",
        "enterprise",
        "/api_skb/v1/companyDetail/sectionInfo",
        auth="business",
        description="Get company recruitment information",
        required=("id",),
        defaults=(("section", "ManageInfo"), *BUSINESS_DEFAULTS),
    ),
}

CAPTURED_OPERATIONS = tuple(SPECS)
