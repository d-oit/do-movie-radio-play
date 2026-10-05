#!/usr/bin/env python3
"""#362 step 1: does F0-band harmonicity separate dialogue (de SRT) from non-speech?
Needs numpy+scipy; run from repo root after scripts/fetch_test_assets.sh."""
import re,subprocess,numpy as np
raw=subprocess.run(["ffmpeg","-v","quiet","-i","testdata/raw/elephants_dream_2006.mp4","-ac","1","-ar","16000","-f","f32le","-"],capture_output=True).stdout
x=np.frombuffer(raw,dtype=np.float32); sr=16000
def t(s):
    h,m,r=s.replace(',','.').split(':'); return int(h)*3600+int(m)*60+float(r)
subs=[(t(a),t(b)) for a,b in re.findall(r'(\d\d:\d\d:\d\d,\d+) --> (\d\d:\d\d:\d\d,\d+)',open('testdata/raw/elephants_dream_2006.de.srt').read())]

from scipy.signal import butter,sosfilt
x=sosfilt(butter(4,1000,'low',fs=sr,output='sos'),x).astype(np.float32)
def run(win,hop_n):
    N=int(win*sr); lo,hi=int(sr/300),int(sr/80)
    H=[];lab=[]
    for i in range(0,len(x)-N,N//2):
        f=x[i:i+N]; f=f-f.mean(); e=(f*f).sum()
        if np.sqrt(e/N)<0.005: H.append(0);lab.append(None);continue
        ac=np.correlate(f,f,'full')[N-1:]/e
        H.append(ac[lo:hi].max()); lab.append(any(a<=i/sr<=b for a,b in subs))
    H=np.array(H)
    # continuity: min over 3 neighbouring frames (sustained voicing)
    C=np.minimum(np.minimum(H,np.roll(H,1)),np.roll(H,-1))
    for name,v in (("raw",H),("sustained",C)):
        S=np.array([v[j] for j,l in enumerate(lab) if l is True]);Nn=np.array([v[j] for j,l in enumerate(lab) if l is False])
        best=max(((th,(S>th).mean()-(Nn>th).mean(),(S>th).mean(),(Nn>th).mean()) for th in np.arange(0.2,0.95,0.05)),key=lambda z:z[1])
        print(win,name,"youden %.3f at %.2f (speech %.2f, nonspeech %.2f)"%(best[1],best[0],best[2],best[3]))
run(0.064,0); run(0.1,0)
