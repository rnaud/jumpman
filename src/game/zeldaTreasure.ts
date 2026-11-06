import type {CourseId} from './courses';
import type {Vec3} from '../sim/math';

export interface TreasureChest { key:string; pos:Vec3; item:number }
export interface WarpPad { course:CourseId; pos:Vec3 }
export interface TreasureReward { name:string; rupees?:number; heal?:boolean }
export const treasureRewards: Readonly<Record<number,TreasureReward>> = {
  0x05:{name:'Fairy Slingshot'}, 0x1f:{name:'Odd Mushroom'}, 0x2b:{name:'Mirror Shield'},
  0x35:{name:'Silver Gauntlets'}, 0x3e:{name:'Piece of Heart'}, 0x40:{name:'Compass'},
  0x41:{name:'Dungeon Map'}, 0x48:{name:'Recovery Heart',heal:true},
  0x4c:{name:'Green Rupee',rupees:1}, 0x4d:{name:'Blue Rupee',rupees:5},
  0x55:{name:'Purple Rupee',rupees:50}, 0x56:{name:'Huge Rupee',rupees:200},
};

export const warpDestinations: ReadonlyArray<{id:CourseId; label:string}> = [
  {id:'sacred_forest_meadow',label:'Sacred Forest Meadow'}, {id:'death_mountain_crater',label:'Death Mountain Crater'},
  {id:'lake_hylia',label:'Lake Hylia'}, {id:'desert_colossus',label:'Desert Colossus'},
  {id:'graveyard',label:'Graveyard'},
];
