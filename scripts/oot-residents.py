"""Room actor IDs, object symbols, and opening lines from the US 1.2 actor overlays."""
from dataclasses import dataclass


@dataclass(frozen=True)
class Resident:
    label: str
    bank: str
    skeleton: str
    idle: str
    greeting: int = 0
    textures: tuple = ()  # Runtime texture segments, starting at 0x08
    scale: float = .01
    motion_bank: str = ''


guard = Resident('Hyrulean Soldier','sd','gEnHeishiSkel','gEnHeishiIdleAnim',0x7002)
zora = Resident('Zora','zo','gZoraSkel','gZoraIdleAnim',0x4006,('gZoraEyeOpenTex',))
goron = Resident('Goron','oF1d_map','gGoronSkel','gGoronAnim_0029A8',0x3002,
                 ('gGoronCsEyeOpenTex','gGoronCsMouthNeutralTex'))
scarecrow = Resident('Bonooru','ka','object_ka_Skel_0065B0','object_ka_Anim_000214',0x40A0)
scrub = Resident('Deku Scrub','hintnuts','gHintNutsSkel','gHintNutsLookAroundAnim',0x109B)
RESIDENTS = {
    0x3C: Resident('Ranch Horse','horse_normal','gHorseNormalSkel','gHorseNormalIdleAnim'),
    0x9A: Resident('Epona','horse_link_child','gChildEponaSkel','gChildEponaIdleAnim',scale=.005),
    0xCB: Resident('Ingo','in','gIngoSkel','gIngoStandingAnim',0x2030,('gIngoEyeOpenTex','gIngoHeadGradient2Tex')),
    0xE7: Resident('Malon','ma1','gMalonChildSkel','gMalonChildIdleAnim',0x2041,('gMalonChildEyeOpenTex','gMalonChildNeutralMouthTex')),
    0x14D: Resident('Kaepora Gaebora','owl','gOwlPerchingSkel','gOwlPerchAnim',0x2064,('gObjOwlEyeOpenTex',),.025),
    0x178:guard, 0xB3:guard, 0x142:guard,
    0x1BC: Resident('Carpenter','daiku','object_daiku_Skel_007958','object_daiku_Anim_007DE0',0x506A),
    0x167: Resident('Man on the Roof','ani','gRoofManSkel','gRoofManIdleAnim',0x5050,('gRoofManEyeOpenTex',)),
    0x13C: Resident('Cucco Lady','ane','gCuccoLadySkel','gObjOsAnim_07D0',0x5036,('gCuccoLadyEyeOpenTex',),motion_bank='object_os_anime'),
    0x132: Resident('Master Craftsman','toryo','object_toryo_Skel_007150','object_toryo_Anim_000E50',0x5028),
    0x16C: Resident('Graveyard Boy','cs','gGraveyardKidSkel','gGraveyardKidIdleAnim',0x2022,('gGraveyardKidEyesOpenTex',)),
    0xED: Resident('Frog','fr','object_fr_Skel_00B498','object_fr_Anim_001534',0,('object_fr_Tex_0059A0','object_fr_Tex_0059A0'),.015),
    0x13E: Resident('Bean Salesman','ms','gBeanSalesmanSkel','gBeanSalesmanEatingAnim',0x405E,scale=.015),
    0x164: Resident('King Zora','kz','gKzSkel','gKzIdleAnim',0x401A,('gKzEyeOpenTex',)),
    0xA1: Resident('Princess Ruto','ru1','gRutoChildSkel','gRutoChildWaitHandsOnHipsAnim',0x402C,('gRutoChildEyeOpenTex','gRutoChildMouthClosedTex')),
    0x124:zora, 0x1CE:zora, 0x1AE:goron, 0x13D:goron, 0x1CA:scarecrow, 0x17B:scarecrow,
    0x138: Resident('Gerudo','ge1','gGerudoWhiteSkel','gGerudoWhiteIdleAnim',0x6000,('gGerudoWhiteEyeOpenTex',)),
    0x186: Resident('Gerudo Guard','gla','gObjectGlaSkel','gObjectGlaIdleAnim',0x6004,('gObjectGlaEyeOpenTex',)),
    0x16A: Resident('Carpet Merchant','js','gCarpetMerchantSkel','gCarpetMerchantIdleAnim',0x6074),
    0x84: Resident('Talon','ta','gTalonSkel','gTalonSleepAnim',0x204B,('gTalonEyeOpenTex','gTalonHeadSkinTex')),
    0x98: Resident('Darunia','du','gDaruniaSkel','gDaruniaIdleAnim',0x301A,('gDaruniaEyeOpenTex','gDaruniaMouthSeriousTex','gDaruniaNoseSeriousTex')),
    0x1C6: Resident('Cow','cow','gCowBodySkel','gCowBodyChewAnim'),
    0x19: Resident('Cucco','niw','gCuccoSkel','gCuccoAnim'),
    0x5A: Resident('Lord Jabu-Jabu','jj','gObjectJjSkel','gObjectJjAnim',0,('gObjectJjEyeOpenTex',),.087),
    0x115: Resident('Skull Kid','skj','gSkullKidSkel','gSkullKidPlayFluteAnim',0x10BC),
    0x195: Resident('Business Scrub','shopnuts','gBusinessScrubSkel','gBusinessScrubLookAroundAnim',0x10A0),
    0x192:scrub, 0x1A3:scrub,
    0x1BF: Resident('Poe Guide','po_field','gPoeFieldSkel','gPoeFieldFloatAnim'),
}


def visible_actor(actor, params):
    # The frog conductor, invisible Ocarina stumps, and Jabu's collision actors have no model.
    return not ((actor == 0xED and params == 0) or (actor == 0x115 and (params & 0xffff)>>10 > 2)
                or (actor == 0x5A and params != -1))


def greeting_for(actor, params, course):
    if actor == 0x14D:
        return {1:0x2064,2:0x2068,3:0x206C,4:0x206F,5:0x2071,6:0x4002,
                7:0x4003,8:0x4004,10:0x6079,11:0x10C0,12:0x10C4}.get((params>>6)&63,0x2064)
    if actor == 0x1AE:
        return {0:0x3011,4:0x300A,5:0x3009,6:0x3008,7:0x3014,8:0x3016,9:0x3017,
                10:0x300E,11:0x3024,12:0x3064}[params&31]
    if actor == 0x1CE:
        return {6:0x4020,7:0x4021}.get(params&63,0x4006)
    if actor == 0x192:
        base = (params>>8)&255
        return 0x1000+base if base else 0
    if actor == 0x178:
        return 0x5063
    if actor == 0xB3:
        return 0x7029 if course=='kakariko_village' else 0x7002
    return {0x124:0x4053,0x17B:0x4076,0x13D:0x3049,0x142:0x702D,
            0x16A:0x6077,0xCB:0x203F}.get(actor,RESIDENTS[actor].greeting)
