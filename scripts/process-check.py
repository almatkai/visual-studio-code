"""Seven stock SDK commands using an owned Rust editor process and recording host."""
import json,pathlib,queue,subprocess,sys,tempfile,threading,uuid
fixture=pathlib.Path(sys.argv[1]).resolve()
production=pathlib.Path(sys.argv[2]).resolve()
v2=pathlib.Path(__file__).resolve().parents[1]
assert set(json.loads(subprocess.check_output([str(production),'--protocol-info']))['commands'])=={'index','open-with-vscode','open-new-window','extensions','install-extension','commandpalette','open-agents-window'}
with tempfile.TemporaryDirectory(prefix='tezbar-vscode-owned-') as tmp:
 root=pathlib.Path(tmp);err=open(root/'stderr.log','wb');p=subprocess.Popen([str(fixture),str(root)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=err);frames=queue.Queue();calls=[]
 def reader():
  for line in p.stdout:frames.put(json.loads(line))
  frames.put(None)
 threading.Thread(target=reader,daemon=True).start()
 def send(f):p.stdin.write(json.dumps(f,separators=(',',':')).encode()+b'\n');p.stdin.flush()
 def call(m,args):
  ident='h:'+str(uuid.uuid4());send({'type':'request','id':ident,'method':m,'params':args})
  while True:
   f=frames.get(timeout=15);assert f is not None,'Unexpected EOF'
   if f.get('type')=='request':
    calls.append(f);result=[str(root/'Project #100%')] if f['method']=='host.selection.read' else ([{'name':'Windsurf','bundleId':'com.exafunction.windsurf','path':str(root/('Custom/Windsurf.exe' if sys.platform=='win32' else 'Custom/Windsurf.app'))}] if f['method']=='host.apps.list' else None)
    send({'type':'response','id':f['id'],'result':result})
   elif f.get('id')==ident:
    assert 'error' not in f,f
    return f['result']
 def open_(sid,command,prefs=None):return call('session.open',{'sessionId':sid,'commandName':command,'preferences':prefs or {}})
 def invoke(sid,ident,**extra):return call('session.call',{'sessionId':sid,'operation':'extension:invoke-action','payload':{'actionId':ident,**extra}})
 def action(v,title,key=None):
  allowed=None
  if key is not None:allowed=next(r['props']['actionIds'] for r in v['root']['children'] if r.get('props',{}).get('id')==key)
  return next(a['id'] for a in v['actions'] if a['title']==title and (allowed is None or a['id'] in allowed))
 def cli():return json.loads((root/'cli-last.json').read_text())
 hello=frames.get(timeout=10);assert hello['id']=='com.tezbar.native.visual-studio-code'
 caps=json.loads((v2/'extension.json').read_text(encoding='utf-8'))['capabilities']
 legacy=root/'legacy-shared.json';legacy_bytes=json.dumps({'pinned':json.dumps([{'folderUri':(root/'Project #100%').as_uri()+'/'}])}).encode();legacy.write_bytes(legacy_bytes)
 call('initialize',{'protocol':1,'extensionId':hello['id'],'platform':'Windows' if sys.platform=='win32' else 'macOS','packageDir':str(v2),'dataDir':str(root/'data'),'tempDir':str(root/'temp'),'grantedCapabilities':caps,'migrationPaths':{'legacy-cache-0':str(legacy)}})
 v=open_('recent','index',{'layout':'grid','closeOtherWindows':True});assert v['root']['type']=='Grid' and not calls
 assert len(json.loads((root/'data/pinned-v1.json').read_text()))==1 and legacy.read_bytes()==legacy_bytes
 key='workspace:vscode-remote://ssh-remote+owned/tmp/custom'
 assert any(r.get('props',{}).get('accessories',[{}])[0].get('tag',{}).get('value')=='fixture-owner' for r in v['root']['children'] if r['type']=='Grid.Item')
 v=invoke('recent',action(v,'Open and Close Other Windows',key))
 if sys.platform=='win32':assert cli()==['--reuse-window','--file-uri','vscode-remote://ssh-remote+owned/tmp/custom']
 else:assert calls[-1]['method']=='host.shell.open' and calls[-1]['params']['target']=='vscode://vscode-remote/ssh-remote+owned/tmp/custom'
 v=invoke('recent',action(v,'Open Project',key))
 if sys.platform=='win32':assert cli()[0]=='--new-window' and cli()[1]=='--file-uri'
 else:assert calls[-1]['params']['target']=='vscode://vscode-remote/ssh-remote+owned/tmp/custom?windowId=_blank'
 v=invoke('recent',action(v,'Pin',key));assert len(json.loads((root/'data/pinned-v1.json').read_text()))==2
 copy=action(v,'Copy Path',key);v=invoke('recent',copy);assert calls[-1]['method']=='host.clipboard.write';before=len(calls);invoke('recent',copy);assert len(calls)==before
 v=invoke('recent',action(v,'Extension Preferences'));assert calls[-1]['method']=='host.settings.open' and calls[-1]['params']['surface']=='settings-extensions'
 v=open_('installed','extensions');v=invoke('installed',action(v,'Update Extension','fixture.test'));assert cli()==['--install-extension','fixture.test','--force']
 v=invoke('installed',action(v,'Open in Browser','fixture.test'));assert calls[-1]['params']['target']=='https://marketplace.visualstudio.com/items?itemName=fixture.test'
 v=invoke('installed',action(v,'Uninstall Extension…','fixture.test'));confirm=action(v,'Confirm Uninstall Extension');assert all(confirm in r['props']['actionIds'] for r in v['root']['children'] if r['type']=='List.Item')
 v=invoke('installed',confirm);assert cli()==['--uninstall-extension','fixture.test'] and not any(r['type']=='List.Item' for r in v['root']['children'])
 v=open_('marketplace','install-extension');assert not any(r['type']=='List.Item' for r in v['root']['children'])
 v=call('session.call',{'sessionId':'marketplace','operation':'extension:search-text-changed','payload':{'searchText':'x'*2049}});assert '2048' in v['message']
 v=open_('palette','commandpalette');assert 'Fixture Run' in json.dumps(v)
 v=invoke('palette',action(v,'Run Command'));assert calls[-1]['method']=='host.shell.open' and '%23' in calls[-1]['params']['target'] and '%25' in calls[-1]['params']['target']
 invoke('palette',action(v,'Create Quicklink'));assert calls[-1]['method']=='host.quicklinks.create'
 v=open_('new','open-new-window');assert v['mode']=='no-view' and cli()==['--new-window']
 v=open_('agents','open-agents-window');assert v['mode']=='no-view' and cli()==['--agents']
 v=open_('insiders-agents','open-agents-window',{'build':'Code - Insiders'});assert v['mode']=='no-view' and cli()==['--agents']
 for build in ['Antigravity','Cursor','Windsurf']:
  before=len(calls);v=open_('agents-'+build,'open-agents-window',{'build':build});assert v['mode']=='no-view'
  if build=='Windsurf':
   assert calls[before]['method']=='host.apps.list';before+=1
  assert calls[before]['method']=='host.shell.open' and str(root) in calls[before]['params']['target']
  if build=='Windsurf':assert calls[before]['params']['target']==str(root/('Custom/Windsurf.exe' if sys.platform=='win32' else 'Custom/Windsurf.app'))
  assert calls[before+1]['method']=='host.window.visibility'
 v=open_('selected','open-with-vscode');assert v['mode']=='no-view' and cli()==['--new-window',str(root/'Project #100%')]
 selection=next(c for c in calls if c['method']=='host.selection.read');assert selection['params']=={'kind':'files','includeDirectories':True,'fallbackToCurrentDirectory':True}
 for sid in ['recent','installed','marketplace','palette','new','selected','agents','insiders-agents','agents-Antigravity','agents-Cursor','agents-Windsurf']:call('session.dispose',{'sessionId':sid})
 p.stdin.close();assert p.wait(timeout=5)==0;err.close()
 print('All seven stock SDK commands passed owned editor/clipboard/settings/selection/Quick Link/EOF process checks')
