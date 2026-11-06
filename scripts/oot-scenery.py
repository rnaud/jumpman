"""Child-day scenery variants selected by the OoT actor overlays."""
from typing import NamedTuple
from math import sin, cos, pi


class Prop(NamedTuple):
    name: str
    bank: str = ''  # An empty bank reuses an existing exported model.
    lists: str = '' # Space-separated display lists in draw order.
    scale: float = .1
    collision: str = ''
    translucent: bool = False


RECOVERY_HEART = Prop("recovery_heart","object_gi_heart","gGiRecoveryHeartDL",.32)
DRAWBRIDGE_CHAIN = Prop('drawbridge_chain','object_spot00_objects','gHyruleFieldCastleDrawbridgeChainsDL',1)
STATIC = {
    0xbf: Prop('crater_smoke','object_spot17_obj','gCraterSmokeConeDL',.1,translucent=True),
    0x4c: Prop('bomb_flower','object_bombf','gBombFlowerLeavesDL gBombFlowerBaseLeavesDL gBombFlowerBombAndSparkDL',.01),
    0x4a: Prop('castle_drawbridge','object_spot00_objects','gHyruleFieldCastleDrawbridgeDL',1,'gHyruleFieldCastleDrawbridgeCol'),
    0x9d: Prop('gravestone','object_haka','gGravestoneStoneDL gGravestoneEarthDL',.1,'gGravestoneCol'),
    0x102: Prop('kakariko_village/windmill_sails'),
    0x107: Prop('milk_crate','object_spot15_obj','gLonLonMilkCrateDL',.1,'gLonLonMilkCrateCol'),
    0xf8: Prop('corral_fence','object_spot15_obj','gLonLonCorralFenceDL',.1,'gLonLonCorralFenceCol'),
    0x11f: Prop('river_waterfall','object_spot03_object','object_spot03_object_DL_000B20 object_spot03_object_DL_000BC0 object_spot03_object_DL_001580',.1,translucent=True),
    0x190: Prop('wasteland_gate','object_spot12_obj','gGerudoFortressWastelandGateDL',.1,'gGerudoFortressWastelandGateCol'),
    0x191: Prop('training_gate','object_spot12_obj','gGerudoFortressGTGShutterDL',.1,'gGerudoFortressGTGShutterCol'),
    0x15c: Prop('goron_vase','object_spot18_obj','gGoronCityVaseDL',.1,'gGoronCityVaseCol'),
    0x1c4: Prop('goron_door','object_spot18_obj','gGoronCityDoorDL',.1,'gGoronCityDoorCol'),
    0x19f: Prop('colossus_wall','object_spot11_obj','gDesertColossusBombableWallDL',1,'gDesertColossusBombableWallCol'),
    0x1a8: Prop('fountain_wall','object_spot08_obj','gZorasFountainBombableWallDL',1,'gZorasFountainBombableWallCol'),
    0x1a9: Prop('crater_wall','object_spot17_obj','gCraterBombableWallDL',.1,'gCraterBombableWallCol'),
    0xcd: Prop('cavern_boulder','object_spot16_obj','gDodongosCavernRockDL',.4),
    0x127: Prop('bombable_boulder','object_bombiwa','object_bombiwa_DL_0009E0'),
    0x1d2: Prop('bronze_boulder','gameplay_field_keep','gSilverRockDL',.4),
    0x19e: Prop('beehive','gameplay_field_keep','gFieldBeehiveDL'),
    0x1a0: Prop('kakariko_village/crate',collision='gLargeCrateCol'),
    0x141: Prop('kokiri_forest/sign'),
}


def prop_for(actor,p):
    if actor == 0x9b and not p & 0x300:
        return Prop('grotto_entrance','gameplay_field_keep','gGrottoDL',.01,translucent=True)
    if actor == 0x9c and p&255 < 3:
        dl,col = (('012A50','012BA4'),('0127C0','0128D8'),('0130B0','0133EC'))[p&255]
        return Prop('grave_monument_'+str(p&255),'object_spot02_objects','object_spot02_objects_DL_'+dl,.1,'object_spot02_objects_Col_'+col)
    if actor == 0x6e and 1 <= p <= 6:
        emblem = ('flash','dark','flame','ice','soul','wind')[p-1]
        return Prop('warp_pad_'+emblem,'object_mjin','gWarpPadBaseDL',1,'gWarpPadCol')
    if actor == 0x12c:
        return Prop('collapsing_platform','object_d_lift','gCollapsingPlatformDL',.05 if p&2 else .1,'gCollapsingPlatformCol')
    if actor == 0x50 and p&255 == 2:
        return Prop('deku_platforms','object_ydan_objects','gDTRisingPlatformsDL',.1,'gDTRisingPlatformsCol')
    if actor == 0x12d:
        part = 'Target' if p&3==2 else 'Post'
        return Prop('hookshot_'+part.lower(),'object_d_hsblock','gHookshot'+part+'DL',.1,'gHookshot'+part+'Col')
    if actor == 0x51:
        if p>>8==1:
            return Prop('deku_ladder','object_ydan_objects','gDTFallingLadderDL',.1,'gDTFallingLadderCol')
        return Prop('deku_spike_log','object_ydan_objects','gDTRollingSpikeTrapDL')
    if actor == 0xb8 and p == 2: # Gerudo Valley uses only this bridge in the child scene.
        return Prop('valley_bridge','object_spot09_obj','gValleyBridgeChildDL',1,'gValleyObjects2Col')
    if actor == 0xb9 and p & 15 in (0,1):
        part = ('Statue','StatueSpear')[p & 15]
        return Prop('goron_'+part.lower(),'object_spot18_obj','gGoronCity'+part+'DL',.1,'gGoronCity'+part+'Col')
    if actor == 0xd5 and p>>8 < 3:
        part = ('WaterTempleGate','WaterTempleKey','HighWater')[p>>8]
        return Prop('lake_'+part.lower(),'object_spot06_objects','gLakeHylia'+part+'DL',
                    1 if p>>8==2 else .1,'gLakeHyliaWaterTempleGateCol' if p>>8==0 else '',p>>8==2)
    if actor == 0x120 and p in (0,1):
        return Prop('domain_falls_'+str(p),'object_spot07_object',
                    ('object_spot07_object_DL_000460','object_spot07_object_DL_000BE0')[p],translucent=True)
    if actor == 0x18d:
        dl,col = (('000840','000918'),('001190','0012C0'))[p & 1]
        return Prop('meadow_gate_'+str(p&1),'object_spot05_objects','object_spot05_objects_DL_'+dl,.1,'object_spot05_objects_Col_'+col)
    if actor == 0x5e:
        style = ('Golden','Timed','Wooden')[p>>12]
        return Prop(style.lower()+'_torch','object_syokudai','g'+style+'TorchDL',1)
    if actor == 0x111:
        return Prop('kakariko_village/pot') if p&256 else Prop('dungeon_pot','gameplay_dangeon_keep','gPotDL',.15)
    if actor == 0x14e:
        return Prop('silver_rock','gameplay_field_keep','gSilverRockDL',.4) if p&1 else Prop('kakariko_village/rock')
    if actor == 0x125:
        return Prop('wild_grass','object_kusa','object_kusa_DL_000140',.4) if p&3 else Prop('kakariko_village/shrub')
    if actor == 0x14f and p == 0:
        return Prop('flowers','gameplay_field_keep','gHanaDL',.01)
    if actor == 0x39 and p&255 in (9,10):
        return Prop('kakariko_village/arrow_sign' if p&255==10 else 'kokiri_forest/sign')
    if actor == 0x77:
        kind = p&255
        if kind < 5: return Prop('kakariko_village/tree')
        if kind < 10: return Prop('oval_tree','object_wood02','object_wood02_DL_007CA0 object_wood02_DL_007D38',1)
        if kind in (11,12,13,14,15,16): return Prop('woodland_bush','object_wood02','object_wood02_DL_000090',1)
    return STATIC.get(actor)


def cluster_offsets(kind):
    if kind == 1:
        scattered = [(40,0x666),(40,0x2ccc),(40,0x5999),(40,0x8666),(20,0xc000),(80,0x1333),
                     (80,0x4000),(80,0x6ccc),(80,0x9333),(80,0xaccc),(80,0xc666),(60,0xe000)]
        return [(r*cos(a*pi/32768),-r*sin(a*pi/32768)) for r,a in scattered]
    ring = [(80*sin(i*pi/4),80*cos(i*pi/4)) for i in range(8)]
    return [(0,0),*ring] if kind==0 else ring


def wood_offsets(kind,yaw):
    """EnWood02 spawns five children and offsets the parent to the sixth point."""
    points = ((707,0x1fff),(525,0x4c9e),(510,0x77f5),(500,0xa5c9),(566,0xd6c3),(141,0xa000))
    for radius,heading in points:
        angle = (heading+yaw+(0x4000 if kind==15 else 0))*pi/32768
        yield radius*sin(angle),radius*cos(angle),heading
