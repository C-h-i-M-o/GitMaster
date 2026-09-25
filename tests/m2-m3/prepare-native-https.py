# -*- coding: utf-8 -*-
"""为既有临时仓库生成仅本机可用的 Apache HTTPS 验收配置。"""
import json
import pathlib
import subprocess
import tempfile

fixtures = json.loads(pathlib.Path("tests/m2-m3/native-sync-fixtures.json").read_text())
root = pathlib.Path(tempfile.mkdtemp(prefix="gitmaster-https-"))
root.chmod(0o700)
git_root = pathlib.Path(fixtures[0]["remote"]).parent.parent
backend = subprocess.check_output(["git", "--exec-path"], text=True).strip()
cert = root / "localhost.pem"
key = root / "localhost.key"
subprocess.run([
    "openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes",
    "-keyout", str(key), "-out", str(cert), "-days", "2",
    "-subj", "/CN=localhost", "-addext", "subjectAltName=IP:127.0.0.1,DNS:localhost",
], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
key.chmod(0o600)
modules = ["mpm_prefork", "unixd", "authz_core", "authz_host", "env", "alias", "cgi", "ssl", "socache_shmcb"]
config = [f'ServerRoot "{root}"', 'Listen 127.0.0.1:18443', 'ServerName localhost',
          f'PidFile "{root}/httpd.pid"', f'ErrorLog "{root}/error.log"',
          'LogLevel warn']
config += [f'LoadModule {name}_module /usr/libexec/apache2/mod_{name}.so' for name in modules]
config += [f'DocumentRoot "{root}"', 'SSLEngine on',
           f'SSLCertificateFile "{cert}"', f'SSLCertificateKeyFile "{key}"',
           f'SetEnv GIT_PROJECT_ROOT "{git_root}"', 'SetEnv GIT_HTTP_EXPORT_ALL 1',
           f'ScriptAlias /git/ "{backend}/git-http-backend/"',
           f'<Directory "{backend}">', 'Require local', 'Options +ExecCGI', '</Directory>']
(root / "httpd.conf").write_text("\n".join(config) + "\n")
xdg = root / "xdg"
(xdg / "git").mkdir(parents=True)
(xdg / "git/config").write_text(f'[http "https://127.0.0.1:18443/"]\n\tsslCAInfo = {cert}\n')
for item in fixtures:
    subprocess.run(["git", "--git-dir", item["remote"], "config", "http.receivepack", "true"], check=True)
metadata = {"root": str(root), "xdg": str(xdg), "config": str(root / "httpd.conf"), "certificate": str(cert)}
pathlib.Path("tests/m2-m3/native-https.json").write_text(json.dumps(metadata, indent=2) + "\n")
print(json.dumps(metadata))
