"""创建四个独立本地远端场景，仅写入新建的临时目录。"""
from pathlib import Path
import json
import subprocess
import tempfile

root = Path(tempfile.mkdtemp(prefix='gitmaster-同步 验收-'))

def git(cwd, *args):
    """固定参数调用系统 Git，失败即停止，不使用 shell。"""
    return subprocess.run(['git', '-C', str(cwd), *args], check=True, capture_output=True, text=True).stdout.strip()

def identity(path):
    """身份只写入该测试仓库配置。"""
    git(path, 'config', 'user.name', 'gitMaster Test')
    git(path, 'config', 'user.email', 'gitmaster-test@example.invalid')

cases = []
for relation in ('equal', 'ahead', 'behind', 'diverged'):
    parent = root / relation
    parent.mkdir()
    remote = parent / 'remote.git'
    seed = parent / 'seed'
    work = parent / '中文 工作目录'
    git(parent, 'init', '--bare', '--initial-branch=main', str(remote))
    git(parent, 'clone', str(remote), str(seed))
    identity(seed)
    (seed / 'base.txt').write_text('共同基线\n', encoding='utf-8')
    git(seed, 'add', 'base.txt')
    git(seed, 'commit', '-m', '建立测试基线')
    git(seed, 'push', '-u', 'origin', 'main')
    git(parent, 'clone', str(remote), str(work))
    identity(work)
    if relation in ('ahead', 'diverged'):
        (work / 'local.txt').write_text('本地提交\n', encoding='utf-8')
        git(work, 'add', 'local.txt')
        git(work, 'commit', '-m', '测试本地领先')
    if relation in ('behind', 'diverged'):
        (seed / 'remote.txt').write_text('远端提交\n', encoding='utf-8')
        git(seed, 'add', 'remote.txt')
        git(seed, 'commit', '-m', '测试远端领先')
        git(seed, 'push', 'origin', 'main')
    assert not git(work, 'status', '--porcelain')
    cases.append({'relation':relation, 'path':str(work), 'remote':str(remote),
                  'head':git(work, 'rev-parse', 'HEAD'),
                  'remoteHead':git(seed, 'rev-parse', 'HEAD'),
                  'upstream':git(work, 'rev-parse', '--abbrev-ref', '@{upstream}')})
Path('tests/m2-m3/native-sync-fixtures.json').write_text(json.dumps(cases,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'root':str(root),'cases':len(cases),'clean':True,'upstreams':[c['upstream'] for c in cases]},ensure_ascii=False))
