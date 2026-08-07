import sys
from pathlib import Path


SCRIPTS_DIR = Path(__file__).parents[1] / "lixiao-workflows" / "scripts"
sys.path.insert(0, str(SCRIPTS_DIR))

import lixiao_config  # noqa: E402


def test_default_config_dir_uses_current_user_home(monkeypatch, tmp_path):
    monkeypatch.delenv("LIXIAO_CONFIG_DIR", raising=False)
    monkeypatch.setattr(lixiao_config.Path, "home", classmethod(lambda cls: tmp_path))

    assert lixiao_config.resolve_config_dir() == tmp_path / ".iyw-claw"
