"""Create eight synthetic cookie databases for the opt-in Windows integration test.

Run from the repository root. Never reads or modifies installed browser profiles.
"""
import json
import sqlite3
import base64
import ctypes
import os
from pathlib import Path

def synthetic_encryption_key():
    if os.name != "nt":
        return {}
    from ctypes import wintypes
    class Blob(ctypes.Structure):
        _fields_ = [("size", wintypes.DWORD), ("data", ctypes.POINTER(ctypes.c_ubyte))]
    data = (ctypes.c_ubyte * 32)(*range(32))
    source, target = Blob(32, data), Blob()
    crypt = ctypes.windll.crypt32.CryptProtectData
    crypt.argtypes = [ctypes.POINTER(Blob), wintypes.LPCWSTR, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p, wintypes.DWORD, ctypes.POINTER(Blob)]
    crypt.restype = wintypes.BOOL
    if not crypt(ctypes.byref(source), None, None, None, None, 1, ctypes.byref(target)):
        raise ctypes.WinError()
    try:
        encoded = base64.b64encode(b"DPAPI" + ctypes.string_at(target.data, target.size)).decode("ascii")
    finally:
        free = ctypes.windll.kernel32.LocalFree
        free.argtypes, free.restype = [ctypes.c_void_p], ctypes.c_void_p
        free(target.data)
    return {"encrypted_key": encoded}

root = Path("qa/multi-cookies/fixtures").resolve()
for browser, count in (("chrome", 6), ("brave", 2)):
    browser_root = root / browser
    for number in range(count):
        database = browser_root / f"Profile {number}" / "Network" / "Cookies"
        database.parent.mkdir(parents=True, exist_ok=True)
        with sqlite3.connect(str(database)) as connection:
            connection.execute("CREATE TABLE IF NOT EXISTS meta (key TEXT, value TEXT)")
            connection.execute("DELETE FROM meta")
            connection.execute("INSERT INTO meta VALUES ('version', '24')")
            connection.execute("CREATE TABLE IF NOT EXISTS cookies (host_key TEXT, name TEXT, value TEXT, encrypted_value BLOB, path TEXT, expires_utc INTEGER, is_secure INTEGER)")
            connection.execute("DELETE FROM cookies")
            connection.execute("INSERT INTO cookies VALUES (?, ?, ?, ?, ?, ?, ?)",
                               ("127.0.0.1", "sessionid", f"SYNTHETIC_{browser}_{number}", b"", "/", 0, 0))
    (browser_root / "Local State").write_text(json.dumps({"os_crypt": synthetic_encryption_key(), "profile": {"info_cache": {
        f"Profile {n}": {"name": f"Test {browser} {n}"} for n in range(count)
    }}}), encoding="utf-8")
print(root)
