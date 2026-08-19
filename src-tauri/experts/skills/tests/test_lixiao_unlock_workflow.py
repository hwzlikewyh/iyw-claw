from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path
from unittest.mock import Mock, call, patch


SCRIPTS_DIR = Path(__file__).parents[1] / "lixiao-workflows" / "scripts"
sys.path.insert(0, str(SCRIPTS_DIR))
SPEC = importlib.util.spec_from_file_location("lixiao_unlock_workflow", SCRIPTS_DIR / "lixiao.py")
assert SPEC and SPEC.loader
LIXIAO = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(LIXIAO)


class CompanyProductUnlockWorkflowTests(unittest.TestCase):
    def test_parser_accepts_contact_source(self) -> None:
        args = LIXIAO.build_parser().parse_args(
            [
                "api",
                "company-products",
                "--id",
                "company-id",
                "--unlock-if-needed",
                "--contact-source",
                "scene_search.searchEcommercePlatformEnterpriseAlibaba_detail",
            ]
        )

        self.assertEqual(
            "scene_search.searchEcommercePlatformEnterpriseAlibaba_detail",
            args.contact_source,
        )

    @patch.object(LIXIAO, "_execute_operation")
    def test_unlocks_then_queries_contacts(self, execute: Mock) -> None:
        hidden = {"data": {"ShopGoodsInfo": {"enableView": False}}}
        visible = {"data": {"ShopGoodsInfo": {"enableView": True}}}
        card = {"data": {"entname": "Example Company"}}
        count = {"data": {"count": 2}}
        contacts = {"data": {"items": [{"name": "Contact"}]}}
        execute.side_effect = [
            hidden,
            {"success": True},
            hidden,
            card,
            count,
            contacts,
            visible,
        ]
        client = Mock()

        result = LIXIAO._run_company_products(
            client,
            {"id": "company-id"},
            dry_run=False,
            unlock_if_needed=True,
            contact_source="custom-source",
        )

        self.assertTrue(result["unlock_performed"])
        self.assertTrue(result["unlock_effective"])
        self.assertTrue(result["contacts_after_unlock"]["performed"])
        self.assertEqual("Example Company", result["contacts_after_unlock"]["company_name"])
        self.assertEqual(
            [
                call(client, "company-products", {"id": "company-id"}, dry_run=False),
                call(client, "company-unlock", {"entityId": "company-id"}),
                call(client, "company-products", {"id": "company-id"}),
                call(client, "company-card", {"id": "company-id"}),
                call(client, "company-contacts-count", {"pid": "company-id"}),
                call(
                    client,
                    "company-contacts",
                    {
                        "pid": "company-id",
                        "entName": "Example Company",
                        "source": "custom-source",
                    },
                ),
                call(client, "company-products", {"id": "company-id"}),
            ],
            execute.call_args_list,
        )

    @patch.object(LIXIAO, "_execute_operation")
    def test_visible_products_do_not_query_contacts(self, execute: Mock) -> None:
        visible = {"data": {"ShopGoodsInfo": {"enableView": True}}}
        execute.return_value = visible

        result = LIXIAO._run_company_products(
            Mock(),
            {"id": "company-id"},
            dry_run=False,
            unlock_if_needed=True,
            contact_source=None,
        )

        self.assertFalse(result["unlock_performed"])
        self.assertEqual("unlock_not_required", result["contacts_after_unlock"]["reason"])
        self.assertEqual(1, execute.call_count)

    @patch.object(LIXIAO, "_execute_operation")
    def test_contact_error_preserves_unlock_result(self, execute: Mock) -> None:
        hidden = {"data": {"ShopGoodsInfo": {"enableView": False}}}
        card = {"data": {"entname": "Example Company"}}
        execute.side_effect = [
            hidden,
            {"success": True},
            hidden,
            card,
            LIXIAO.LixiaoError("contact permission denied", code="forbidden"),
            hidden,
        ]

        result = LIXIAO._run_company_products(
            Mock(),
            {"id": "company-id"},
            dry_run=False,
            unlock_if_needed=True,
            contact_source=None,
        )

        self.assertTrue(result["unlock_performed"])
        self.assertEqual("forbidden", result["contacts_after_unlock"]["error"]["code"])

    @patch.object(LIXIAO, "_execute_operation")
    def test_dry_run_describes_contact_queries(self, execute: Mock) -> None:
        execute.side_effect = [
            {"operation": "company-products"},
            {"operation": "company-unlock"},
            {"operation": "company-card"},
            {"operation": "company-contacts-count"},
            {"operation": "company-products"},
        ]

        result = LIXIAO._run_company_products(
            Mock(),
            {"id": "company-id"},
            dry_run=True,
            unlock_if_needed=True,
            contact_source=None,
        )

        plan = result["contacts_after_unlock"]
        self.assertEqual("company-contacts", plan["contacts"]["operation"])
        self.assertEqual(
            "company-products", result["final_detail_after_contacts"]["operation"]
        )
        self.assertEqual(5, execute.call_count)


if __name__ == "__main__":
    unittest.main()
