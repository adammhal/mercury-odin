import asyncio, json, os, re, urllib.request, urllib.parse, websockets
ENG="http://127.0.0.1:47800"
def jget(u): return json.load(urllib.request.urlopen(urllib.request.Request(u,headers={"User-Agent":"Mercury"}),timeout=60))
def norm(s): return re.sub(r"[^a-z0-9]","",s.lower().replace("™","").replace("®",""))
games=json.load(open(r"C:\Users\adamm\import_games.json",encoding="utf-8"))
games += [  # shortcuts that were already in Steam before the import
 {"name":"Firewatch","exe":r"C:\Games\Firewatch\Firewatch.exe"},
 {"name":"Portal 2","exe":r"C:\Users\adamm\Desktop\Everything Extra\Game Folders\Portal_2\portal2.exe"},
 {"name":"Slay the Spire 2","exe":r"C:\Users\adamm\Desktop\Everything Extra\Game Folders\Slay The Spire 2 Cracked\Slay the Spire 2\SlayTheSpire2.exe"},
 {"name":"Gamble With Your Friends","exe":r"C:\Users\adamm\Desktop\Everything Extra\Game Folders\Gamble.With.Your.Friends.v1.0.11-OFME\Gamble With Your Friends\Gamble With Your Friends.exe"},
]
alias={"Slay the Spire 2":"SlayTheSpire2","Portal 2":"portal2"}   # the names those shortcuts have in Steam
def steam_appid(name):
    r=jget(ENG+"/steam/search?q="+urllib.parse.quote(name)); n=norm(name)
    for a in r:
        if norm(a["name"])==n: return a["appid"]
    for a in r:
        m=norm(a["name"])
        if (m.startswith(n) or n.startswith(m)) and not re.search(r"points|dlc|soundtrack|pack$|bundle|demo|pre-purchase",a["name"].lower()): return a["appid"]
    return None
async def main():
    tabs=json.load(urllib.request.urlopen("http://127.0.0.1:8080/json"))
    ws=await websockets.connect([t for t in tabs if t["title"]=="SharedJSContext"][0]["webSocketDebuggerUrl"],max_size=None)
    await ws.send(json.dumps({"id":1,"method":"Runtime.evaluate","params":{"expression":"JSON.stringify(Array.from(appStore.allApps).filter(x=>x.app_type===1073741824).map(x=>[x.appid,x.display_name]))","returnByValue":True}}))
    while True:
        r=json.loads(await ws.recv())
        if r.get("id")==1: break
    await ws.close()
    shortcuts={norm(n):a for a,n in json.loads(r["result"]["result"]["value"])}
    items=[]; problems=[]
    for g in games:
        sid=shortcuts.get(norm(alias.get(g["name"],g["name"])))
        aid=steam_appid(g["name"])
        if not sid: problems.append(f"no Steam shortcut found for {g['name']}"); continue
        if not aid: problems.append(f"no Steam store match for {g['name']}"); continue
        items.append({"appid":aid,"name":g["name"],"exe":g["exe"],"shortcut_id":sid})
    req=urllib.request.Request(ENG+"/library/register",data=json.dumps({"games":items}).encode(),headers={"Content-Type":"application/json"})
    out=json.load(urllib.request.urlopen(req,timeout=600))
    print("added to Mercury's library:",len(out["added"])); print(", ".join(out["added"]))
    print("\n".join(problems))
asyncio.run(main())
