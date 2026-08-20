import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent / "src"))
os.environ.setdefault("CLOISON_OFFLINE", "1")
