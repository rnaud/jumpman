import { approachF32, atan2s, clamp, coss, dist3D, sins, toS16 } from '../math';
import type { ObjectContext, SimObject } from '../object';
import { ACT_FLAG_AIR, ACT_FLAG_INTANGIBLE } from '../state';

export function initRacer(o: SimObject): void {
  o.scale=4;
  o.vars.animation=3;
  Object.assign(o.vars,{waypoint:0,airFrames:0,weightedSpeed:0,cooldown:0});
}

function turn(o: SimObject, target: number, speed: number): boolean {
  const delta=toS16(target-o.moveYaw);
  o.moveYaw=o.faceYaw=toS16(o.moveYaw+clamp(delta,-speed,speed));
  return o.moveYaw===toS16(target);
}

// cur_obj_move_standard(78): the racer uses zero drag, gravity -8 and 5% bounce.
function move(o: SimObject, ctx: ObjectContext): void {
  const next:[number,number,number]=[o.pos[0]+o.forwardVel*sins(o.moveYaw),o.pos[1],o.pos[2]+o.forwardVel*coss(o.moveYaw)];
  o.hitWall=!!ctx.world.resolveAndReturnWallCollisions(next,10,300);
  const floor=ctx.world.findFloor(...next);
  if(floor.surface && (floor.height<o.pos[1]+5 || floor.surface.normal[1]>Math.cos(78*Math.PI/180))) {
    o.pos[0]=next[0]; o.pos[2]=next[2];
  }
  const ground=ctx.world.findFloor(...o.pos);
  o.vel[1]=Math.max(-78,o.vel[1]-8);
  o.pos[1]+=o.vel[1];
  o.grounded=!!ground.surface && o.pos[1]<ground.height;
  if(o.grounded){o.pos[1]=ground.height;o.vel[1]*=-0.05;}
}

export function racePenguin(o: SimObject,ctx: ObjectContext): void {
  const race=ctx.penguinRace;
  if(!race)return;
  const m=ctx.mario, t=o.actionTimer++;
  const near = Math.hypot(m.pos[0]-o.pos[0],m.pos[2]-o.pos[2]) < 400
    && o.pos[1] < m.pos[1]+160 && m.pos[1] < o.pos[1]+400 && !(m.action & (ACT_FLAG_AIR|ACT_FLAG_INTANGIBLE));
  switch(o.action){
    case 0:
      if(t>o.vars.cooldown && near && o.pos[1]<=m.pos[1]+1){
        ctx.dialogue('Race me down the slide? A to race, B to decline.');
        o.setAction(1);
      }
      break;
    case 1:
      if(o.vars.answer===1){o.vel[1]=60;o.setAction(2);}
      else if(o.vars.answer===-1){o.vars.cooldown=60;o.setAction(0);}
      break;
    case 2:
      turn(o,0x4000,2500);
      if(t===50)ctx.racerSound(o.pos,'gun');
      if(t>50){o.forwardVel=20;o.setAction(3);}
      break;
    case 3: {
      const [id,x,y,z]=race.path[o.vars.waypoint];
      const [,nx,ny,nz]=race.path[o.vars.waypoint+1];
      const dx=nx-o.pos[0],dy=ny-o.pos[1],dz=nz-o.pos[2];
      if((nx-x)*dx+(ny-y)*dy+(nz-z)*dz<=0 && ++o.vars.waypoint===race.path.length-1){
        o.setAction(4);
      } else {
        ctx.racerSound(o.pos,'slide');
        const lead=o.pos[1]-m.pos[1];
        const slow=lead<100 || id>=35;
        o.vars.weightedSpeed=approachF32(o.vars.weightedSpeed,slow?-500:1000,30,100);
        o.forwardVel=approachF32(o.forwardVel,clamp((o.vars.weightedSpeed+lead)*0.1,id>=35?60:70,150),0.4,0.4);
        turn(o,atan2s(dz,dx),Math.trunc(15*o.forwardVel));
        o.vars.animation=1;
        if(dist3D(m.pos,race.finish)<1000 && m.pos[2]<race.finish[2])o.vars.won=1;
      }
      o.vars.airFrames=m.action & ACT_FLAG_AIR ? o.vars.airFrames+1 : 0;
      if(o.vars.airFrames>60 || dist3D(m.pos,race.shortcut)<500)o.vars.cheated=1;
      break;
    }
    case 4:
      if(o.forwardVel && t>5 && o.hitWall){ctx.racerSound(o.pos,'impact');o.forwardVel=0;o.vars.animation=2;o.vars.stopped=t;}
      if(!o.forwardVel && t-o.vars.stopped>=25)o.setAction(5);
      break;
    case 5:
      if(turn(o,0,200)){
        o.forwardVel=0; o.vars.animation=3;
        if(near){
          if(o.vars.won && !o.vars.cheated){
            ctx.dialogue('You broke my record! I award you the gold medal!');
            ctx.penguinReward([o.pos[0],o.pos[1]+200,o.pos[2]]);
          } else ctx.dialogue(o.vars.won ? 'No shortcuts in this race!' : 'You lost! Try again next time.');
          o.setAction(6);
        }
      } else {o.forwardVel=4;o.vars.animation=0;}
      break;
  }
  move(o,ctx);
  const radius=o.scale===8?350:200, dx=m.pos[0]-o.pos[0],dz=m.pos[2]-o.pos[2],d=Math.hypot(dx,dz);
  if(d<radius && Math.abs(m.pos[1]-o.pos[1])<(o.scale===8?250:200)){
    m.pos[0]+=dx*(radius-d)/radius;m.pos[2]+=dz*(radius-d)/radius;
  }
}
