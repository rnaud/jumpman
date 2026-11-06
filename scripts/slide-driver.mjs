// Follow the decomp's racing line with stick inputs, aiming ahead of Mario on bends.
export async function installSlideDriver(page) {
  await page.evaluate(()=>{
    window.driveMountainSlide=(g,tick,racing=false)=>{
      const m=g.mario;
      const path=g.playground.objects.manager.penguinRace.path.map(p=>p.slice(1));
      path.splice(29,0,[6400,0,-5700]);
      const start=m.respawns;
      let i=1;
      for(let t=0;t<5000 && i<path.length;t++){
        const a=path[i-1],b=path[i];
        const dx=b[0]-a[0],dz=b[2]-a[2],len=Math.hypot(dx,dz);
        const u=((m.pos[0]-a[0])*dx+(m.pos[2]-a[2])*dz)/(len*len);
        if(u>=1 || Math.hypot(m.pos[0]-b[0],m.pos[2]-b[2])<180){i++;continue}
        let ahead=Math.max(0,u*len)+450;
        let k=i-1;
        while(k<path.length-2){
          const length=Math.hypot(path[k+1][0]-path[k][0],path[k+1][2]-path[k][2]);
          if(ahead<=length)break;
          ahead-=length;k++;
        }
        const p=path[k],q=path[k+1],u2=Math.min(1,ahead/Math.hypot(q[0]-p[0],q[2]-p[2]));
        const goal=Math.atan2(p[0]+u2*(q[0]-p[0])-m.pos[0],p[2]+u2*(q[2]-p[2])-m.pos[2]);
        const heading=Math.atan2(m.vel[0],m.vel[2]),err=Math.atan2(Math.sin(goal-heading),Math.cos(goal-heading));
        let angle=m.action&0x40000 ? heading+Math.max(-1.9,Math.min(1.9,err*5)) : goal;
        if(!racing && m.action&0x40000 && Math.abs(m.forwardVel)>60) {
          angle=heading+Math.sign(err)*(Math.PI-Math.min(1.5,Math.abs(err)*5));
        }
        tick([m.pos[0]+1000*Math.sin(angle),0,m.pos[2]+1000*Math.cos(angle)]);
        if(m.respawns!==start)throw Error('Mario fell at slide waypoint '+i);
      }
      if(i<path.length)throw Error('Slide traversal stalled at waypoint '+i);
      return i;
    };
  });
}
