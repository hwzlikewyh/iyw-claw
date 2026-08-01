MARKET_LABELS = {"domestic": "内销", "export": "外销"}
ACTIVITY_LABELS = {
    "sales_hiring": "销售招聘",
    "designer_hiring": "设计招聘",
    "exhibition": "展会活动",
    "shop_update": "店铺更新",
    "copyright_work": "版权作品",
}
SECTION_LABELS = {
    "activities": "活动证据",
    "products": "产品",
    "contacts": "联系人",
    "materials": "销售资料",
}
ACTION_LABELS = {
    "crm_claim_or_create": "客户管理系统捞取或新建客户",
    "notify_sales": "通知负责销售",
    "crm_writeback": "回写客户管理系统",
}
DECISION_LABELS = {
    "eligible_new": "客户管理系统无记录，可新增",
    "eligible_unowned": "客户管理系统未分配，可跟进",
    "crm_unverified": "客户管理系统查询失败，待核验",
    "crm_ambiguous": "客户管理系统存在疑似记录，待复核",
    "crm_review": "客户管理系统状态待复核",
    "skip_protected_star": "客户管理系统保护客户，不跟进",
    "skip_owned": "客户管理系统已有负责人，不跟进",
}
PACKAGE_STATUS_LABELS = {
    "complete": "资料完整",
    "incomplete": "资料不完整",
    "skipped": "已跳过",
    "review": "待复核",
}
ACTION_STATUS_LABELS = {"pending": "待处理", "completed": "已完成"}
MATERIAL_LABELS = {
    "exhibition_report": "展会报告",
    "trend_theme": "趋势主题",
    "retail_image": "卖场图片",
    "catalog_image": "目录图片",
    "pattern_poster": "爆款图案海报",
    "ai_image": "人工智能图片",
}
