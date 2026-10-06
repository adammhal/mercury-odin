import asyncio, json, os, re, sys, base64, struct, urllib.request, urllib.parse, websockets
ENG="http://127.0.0.1:47800"
KEY=json.load(open(os.path.join(os.environ["APPDATA"],"Mercury","config.json"),encoding="utf-8-sig")).get("sgdb_key","")
GRID=r"C:\Program Files (x86)\Steam\userdata\1075839834\config\grid"
VDF=r"C:\Program Files (x86)\Steam\userdata\1075839834\config\shortcuts.vdf"
games=json.load(open(r"C:\Users\adamm\import_games.json",encoding="utf-8"))
DRY="--apply" not in sys.argv
def jget(u,h=None,t=60):
    return json.load(urllib.request.urlopen(urllib.request.Request(u,headers=h or {"User-Agent":"Mercury"}),timeout=t))
def jpost(u,b): return json.load(urllib.request.urlopen(urllib.request.Request(u,data=json.dumps(b).encode(),headers={"Content-Type":"application/json"}),timeout=120))
def sg(path): return jget("https://www.steamgriddb.com/api/v2"+path,{"Authorization":"Bearer "+KEY,"User-Agent":"Mercury"},30)["data"]
def norm(s): return re.sub(r"[^a-z0-9]","",s.lower().replace("™","").replace("®",""))
def img(url):
    resp=urllib.request.urlopen(urllib.request.Request(url,headers={"User-Agent":"Mercury"}),timeout=60); ct=resp.headers.get("content-type","")
    return ("png" if "png" in ct else "webp" if "webp" in ct else "jpg"), base64.b64encode(resp.read()).decode()
SLOT_Q={0:"/grids/game/{g}?dimensions=600x900,342x482,660x930",1:"/heroes/game/{g}",2:"/logos/game/{g}",3:"/grids/game/{g}?dimensions=920x430,460x215"}
def steam_match(name):
    try: r=jget(ENG+"/steam/search?q="+urllib.parse.quote(name))
    except Exception: return None
    n=norm(name)
    for a in r:
        if norm(a["name"])==n: return a
    for a in r:
        m=norm(a["name"])
        if (m.startswith(n) or n.startswith(m)) and not re.search(r"points|dlc|soundtrack|pack$|bundle|demo",a["name"].lower().replace(norm(name),"")): return a
    return None
def sgdb_id(name):
    try:
        h=sg("/search/autocomplete/"+urllib.parse.quote(name)); 
        n=norm(name)
        for x in h:
            if norm(x["name"])==n or norm(x["name"]).startswith(n): return x["id"]
        return h[0]["id"] if h else None
    except Exception: return None
def get_art(name):
    app=steam_match(name); assets={}; src=[]
    if app:
        try:
            for t,ext,b in jpost(ENG+"/art/resolve",{"appid":app["appid"],"choices":{}})["assets"]: assets[t]=(ext,b)
            src.append("steam:"+app["name"])
        except Exception as e: src.append("steam art failed")
    missing=[t for t in range(4) if t not in assets]
    if missing and KEY:
        gid=sgdb_id(name)
        if gid:
            for t in missing:
                try:
                    l=sg(SLOT_Q[t].format(g=gid))
                    if l: ext,b=img(l[0]["url"]); assets[t]=(ext,b)
                except Exception: pass
            src.append("sgdb")
    return app,assets,"+".join(src) or "none"
def vdf_entries():
    d=open(VDF,'rb').read(); ents=re.split(rb'\x00\d+\x00\x02appid\x00',d)[1:]; out=[]
    for e in ents:
        f=lambda k:(lambda m:m.group(1).decode('utf8','replace') if m else '')(re.search(rb'\x01'+k+rb'\x00(.*?)\x00',e,re.I|re.S))
        out.append((struct.unpack('<I',e[:4])[0],f(b'AppName'),f(b'Exe').strip('"')))
    return out
existing={os.path.normcase(e[2]) for e in vdf_entries()}
report=[]
async def main():
    ws=None
    if not DRY:
        tabs=json.load(urllib.request.urlopen("http://127.0.0.1:8080/json"))
        url=[t for t in tabs if t["title"]=="SharedJSContext"][0]["webSocketDebuggerUrl"]
        ws=await websockets.connect(url,max_size=None)
    n=0
    async def ev(x):
        nonlocal n; n+=1
        await ws.send(json.dumps({"id":n,"method":"Runtime.evaluate","params":{"expression":x,"awaitPromise":True,"returnByValue":True}}))
        while True:
            r=json.loads(await ws.recv())
            if r.get("id")==n: return r
    for g in games:
        name,exe=g["name"],g["exe"]
        if not os.path.exists(exe): report.append(f"MISSING FILE  {name}  ({exe})"); continue
        if os.path.normcase(exe) in existing: report.append(f"already in Steam  {name}"); continue
        app,assets,src=get_art(name)
        if DRY: report.append(f"would add {name}  art slots {sorted(assets)} from {src}"); continue
        d=os.path.dirname(exe)
        r=await ev(f"SteamClient.Apps.AddShortcut({json.dumps(name)},{json.dumps(exe)},{json.dumps(d)},'')")
        sid=r.get("result",{}).get("result",{}).get("value")
        if not sid: report.append(f"FAILED to add {name}: {r}"); continue
        await ev(f"SteamClient.Apps.SetShortcutName({sid},{json.dumps(name)})")
        for t,(ext,b) in assets.items():
            await ev(f"SteamClient.Apps.SetCustomArtworkForApp({sid},{json.dumps(b)},{json.dumps(ext)},{t})")
        report.append(f"added {name}  art slots {sorted(assets)} from {src}")
    if ws: await ws.close()
asyncio.run(main())
print("\n".join(report))
