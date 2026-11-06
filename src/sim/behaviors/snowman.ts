import { atan2s, clamp, dist3D, toS16, type Vec3 } from '../math';
import { INTERACT_DAMAGE, OBJ_COL_FLAG_NO_Y_VEL, type ObjectContext, type SimObject } from '../object';
import { ACT_FLAG_AIR, ACT_FLAG_INTANGIBLE } from '../state';

const junction: Vec3 = [-4230,-1344,1813];
const checkpoint: Vec3 = [-402,461,-2898];

export function initSnowmanBody(o: SimObject): void {
  if (o.param===-1) {
    o.scale=1;
    o.graphYOffset=180;
    o.vars.pitch=0;
    o.visible=false;
    return;
  }
  o.scale=0.4;
  o.graphYOffset=72;
  o.gravity=10;
  o.buoyancy=2;
  o.friction=0.999;
  o.vars.point=o.vars.pitch=o.vars.checked=0;
  o.hitbox.interactType=INTERACT_DAMAGE;
  o.hitbox.damageOrCoinValue=3;
  o.hitbox.radius=84;
  o.hitbox.height=140;
}

export function initSnowmanHead(o: SimObject): void {
  o.scale=0.7;
  o.graphYOffset=110;
  o.gravity=5;
  o.friction=0.999;
  o.buoyancy=2;
}

function turn(o:SimObject,target:number):void {
  o.moveYaw=toS16(o.moveYaw+clamp(toS16(target-o.moveYaw),-0x400,0x400));
}

function push(o:SimObject,ctx:ObjectContext,radius:number,height:number):void {
  const m=ctx.mario, dx=m.pos[0]-o.pos[0],dz=m.pos[2]-o.pos[2],d=Math.hypot(dx,dz);
  if(d<radius && Math.abs(m.pos[1]-o.pos[1])<height){
    m.pos[0]+=dx*(radius-d)/radius;
    m.pos[2]+=dz*(radius-d)/radius;
  }
}

export function snowmanBody(o: SimObject,ctx:ObjectContext):void {
  const path=ctx.snowmanPath;
  if(o.param===-1 || !path)return;
  if(!o.vars.placed) {
    const floor=ctx.world.findFloor(...o.home);
    if(floor.surface)o.home[1]=o.pos[1]=floor.height;
    o.vars.placed=1;
  }
  const frame=o.actionTimer++;
  if(!o.vars.checked && dist3D(ctx.mario.pos,checkpoint)<800)o.vars.checked=1;
  if(o.action===0){
    if(o.distanceToMario<400 && !(ctx.mario.action & (ACT_FLAG_AIR|ACT_FLAG_INTANGIBLE))){
      ctx.dialogue('I need a good head on my shoulders! Please follow me.');
      o.forwardVel=10;
      o.setAction(1);
    }
  }else if(o.action===1 || o.action===2){
    o.stepPhysics(ctx.world, o.hitbox.radius);
    o.forwardVel=Math.min(70,o.forwardVel);
    if(o.action===1){
      const last=path[o.vars.point],next=path[o.vars.point+1];
      const dx=next[0]-o.pos[0],dy=next[1]-o.pos[1],dz=next[2]-o.pos[2];
      turn(o,atan2s(dz,dx));
      if(dx*(next[0]-last[0])+dy*(next[1]-last[1])+dz*(next[2]-last[2])<=0 && ++o.vars.point===path.length-1){
        o.vars.target=o.vars.checked && Math.abs(toS16(o.angleToMario-o.moveYaw))<0x2000 ? o.angleToMario : o.moveYaw;
        o.setAction(2);
      }
    }else{
      turn(o,o.vars.target);
      if(dist3D(o.pos,junction)<300){
        o.moveYaw=atan2s(junction[2]-o.pos[2],junction[0]-o.pos[0]);
        o.vel[1]=80;o.forwardVel=15;o.setAction(3);
        const head=ctx.findByBehavior('snowmanHead')!;
        head.setAction(2);head.vel[1]=100;
        ctx.snowmanEvent(o.pos,'bounce');
      }else if(frame===200){
        o.visible=false;
        o.hitbox.interactType=0;
        o.pos=[...o.home];
        o.setAction(5);
        return;
      }
    }
    o.vars.pitch=toS16(o.vars.pitch+Math.trunc(o.forwardVel*100/o.scale));
    o.scale=Math.min(1,o.scale+o.forwardVel*0.0001);
    o.hitbox.radius=210*o.scale;
    o.hitbox.height=350*o.scale;
    o.graphYOffset=180*o.scale;
    o.faceYaw=o.moveYaw;
    ctx.snowmanEvent(o.pos,'roll');
  }else if(o.action===3){
    o.stepPhysics(ctx.world,o.hitbox.radius);
    if(o.grounded){
      o.pos[0]=junction[0];o.pos[2]=junction[2];o.forwardVel=0;
      if(Math.trunc(o.vel[1])===0){o.hitbox.interactType=0;o.setAction(4);}
    }
  }else if(o.action===4)push(o,ctx,210,550);
  else if(o.distanceToMario>=3000){
    o.reactivate();o.moveYaw=o.faceYaw=0;o.setAction(0);initSnowmanBody(o);
  }
}

export function snowmanHead(o:SimObject,ctx:ObjectContext):void {
  if(!o.vars.placed) {
    const floor=ctx.world.findFloor(...o.home);
    if(floor.surface)o.pos[1]=floor.height;
    o.vars.placed=1;
  }
  if(o.action===0 || o.action===4){
    const m=ctx.mario;
    if(o.distanceToMario<(o.action===4?700:400) && !(m.action & (ACT_FLAG_AIR|ACT_FLAG_INTANGIBLE))
        && Math.abs(toS16(o.faceYaw-m.faceAngle[1]-0x8000))<0x1000 && Math.abs(toS16(o.moveYaw-o.angleToMario))<0x1000){
      if(o.action===4){
        ctx.dialogue('Perfect! What a great new body! Here is a present for you.');
        ctx.snowmanEvent(o.pos,'reward');
      }else ctx.dialogue('I need a body! Have you seen one rolling by?');
      o.setAction(1);
    }
  }else if(o.action===2){
    if(o.stepPhysics(ctx.world,0)&OBJ_COL_FLAG_NO_Y_VEL)o.setAction(3);
  }else if(o.action===3){
    o.stepPhysics(ctx.world,0);
    if(o.pos[1]<-994){o.pos[1]=-994;o.setAction(4);ctx.snowmanEvent(o.pos,'assembled');}
  }
  push(o,ctx,180,150);
}
