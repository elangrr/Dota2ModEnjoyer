"""Makes the two files of numbers the app's Draft and Builds pages work from.

draft.json: how well each hero does in each of the five roles, for each of the eight ranks,
how well it does against and beside each other hero, for each pair of ranks, and how its lanes
go in each role, against the heroes it meets there and beside the one it shares them with.

builds.json: what the players of each hero buy and skill, in each role the hero is played in,
for each pair of ranks: the items they start with, the items they go on to buy and when,
boots among them, and which talents are taken. And from single matches of Divine and
Immortal players, some hundreds for each hero and role: the order the skill points are put
in, point by point, and which items are bought more often against which enemy hero. And from
OpenDota (https://www.opendota.com), the pro matches of the patch: what the pros who play a
hero in each role buy, such as the Bottle that nearly every pro mid buys.

    python publish/make_numbers.py <folder to write the two files in>
    python publish/make_numbers.py --check        # tries the sums below on made-up numbers

The numbers are STRATZ's (https://stratz.com), asked for through its API with a token that a
Steam login there gives. The token is read from the environment variable STRATZ_TOKEN or, where
that is not set, from the file .dme/stratz.key in the home folder. It goes nowhere but to
STRATZ, and never into the file that is written. OpenDota is asked without a token, one
question in all. Where it cannot be reached, the file is written without what pros buy.

About sixty questions are asked for draft.json, each for all heroes at once. For
builds.json one is asked for each hero and pair of ranks, about five hundred, and one for
each hero and role, about three hundred, each of which fetches some hundreds of matches.
All of it takes about twenty minutes.
The files are described in docs/SPEC.md, sections 5 and 6; the app refuses one that is not of
that shape.

To try the script out without asking all of that, set DME_FEW_HEROES to a number: builds are
then made for that many heroes only.
"""

import collections
import json
import math
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

API = "https://api.stratz.com/graphql"
# What STRATZ says when its database is full for a while ("the analytics database is at
# capacity. Please retry in a moment."): the same question is asked again after BUSY_WAIT
# seconds, up to BUSY_TRIES times in one question.
CAPACITY = "at capacity"
BUSY_WAIT = 60
BUSY_TRIES = 10
# OpenDota's database of pro matches, which takes a question in SQL.
PRO_API = "https://api.opendota.com/api/explorer?sql="

# The pairs of ranks, in the order the file has them. STRATZ counts hero against hero for a
# pair as a whole (the first name), and wins by role for each of its two ranks (the two names
# after it). The eight ranks of the file are those, in this order.
PAIRS = [
    ("HERALD_GUARDIAN", ["HERALD", "GUARDIAN"]),
    ("CRUSADER_ARCHON", ["CRUSADER", "ARCHON"]),
    ("LEGEND_ANCIENT", ["LEGEND", "ANCIENT"]),
    ("DIVINE_IMMORTAL", ["DIVINE", "IMMORTAL"]),
]
# The five roles, in the order the file has them: carry, mid, offlane, soft support, hard support.
ROLES = ["POSITION_1", "POSITION_2", "POSITION_3", "POSITION_4", "POSITION_5"]

WEEK = 7 * 24 * 3600
# The weeks that are added up: those since the newest patch, but no more and no fewer than these.
MOST_WEEKS = 4
LEAST_WEEKS = 2
# How many matches of a pair of heroes it takes to believe half of what they show. A pair that
# met less often is pulled towards "no difference", so that luck in a few matches is not taken
# for a counter.
DOUBT = 400
# The most that is said of a pair, in tenths of a percent.
MOST = 300
# A file with fewer heroes than this is not written: something went wrong.
LEAST_HEROES = 100
# The most bytes the app takes of each file (NUMBERS_LIMIT in crates/dme-core/src/draft.rs,
# BUILDS_LIMIT in builds.rs). A larger file is not written: every app that is out would refuse it.
LIMITS = {"draft.json": 4 * 1024 * 1024, "builds.json": 8 * 1024 * 1024}

# A build is made for a role that has at least this part of a hero's matches in a pair of
# ranks, for no more than this many roles, and only from at least this many matches.
PLAYED_SHARE = 0.10
MOST_ROLES = 3
LEAST_MATCHES = 150
# What is in a build: an item or a skill that at least this part of the players have, and no
# more than this many of each kind.
START_SHARE, MOST_START = 0.15, 8
BOOTS_SHARE, MOST_BOOTS = 0.05, 4
ITEM_SHARE, MOST_ITEMS = 0.03, 30
# A second, third or fourth copy of an item is counted where at least this part of those who
# start with the item have that many, as with three Iron Branches.
COPY_SHARE = 0.50

# How many single matches are fetched for a hero in a role, and how few of them still say
# in which order the skill points are put.
SAMPLE = 600
LEAST_SAMPLE = 30
# The skill points of an order: one for each level up to 16, then those of the levels 18, 20
# and 25. A point is said while at least this part of the matches got as far.
MOST_POINTS = 19
GOT_SO_FAR = 0.25
# The next point is what most of those do who did the same up to there, while there are at
# least this many of them, and what most of everybody does where there are fewer.
LEAST_THE_SAME = 10
# An item is said to be bought more often against a hero where, added up over all heroes
# that met it, it was expected to be bought at least this many times, was bought more often
# than luck would have it by this many times the spread, and that is at least this many
# times as often as usual. No more than MOST_TIMES is said, and no more than MOST_AGAINST
# items for a hero. DOUBT_BOUGHT pulls what few purchases show towards "as usual".
LEAST_EXPECTED = 8
SURE = 3.0
LEAST_TIMES = 1.25
MOST_TIMES = 5.0
MOST_AGAINST = 14
DOUBT_BOUGHT = 15

# What pros buy in a role is said of a hero that at least this many pros played in it in the
# patch, and of an item at least this part of them bought. The app shows what most buy.
LEAST_PROS = 8
PRO_SHARE = 0.30

# Items whose purchases STRATZ loses, so that its numbers of them are not to be gone by. Of
# five ranked matches whose mid player bought a Bottle in the second minute by OpenDota's
# records, and in four of them still had it at the end, STRATZ had a Bottle in none: 5 of 100
# Storm Spirits by its numbers, where pros, and ranked players by OpenDota's records, buy one
# nearly always. What pros buy (OpenDota) is what says whether to have it.
LOST_BY_STRATZ = {"item_bottle"}


def token():
    given = os.environ.get("STRATZ_TOKEN", "").strip()
    if given:
        return given
    path = os.path.join(os.path.expanduser("~"), ".dme", "stratz.key")
    try:
        with open(path, encoding="utf-8") as file:
            return file.read().strip()
    except OSError:
        sys.exit("No STRATZ token: set STRATZ_TOKEN, or put the token in " + path)


class Asker:
    """Asks STRATZ, and counts the questions."""

    def __init__(self, secret):
        self.secret = secret
        self.asked = 0

    def ask(self, query):
        body = json.dumps({"query": query}).encode("utf-8")
        headers = {"Authorization": "Bearer " + self.secret, "User-Agent": "STRATZ_API", "Content-Type": "application/json"}
        # `attempt` counts tries against refusals and lost connections; a full database
        # (`busy`) is counted apart, so that waiting for it does not use those up.
        attempt, busy = -1, 0
        while attempt < 3:
            attempt += 1
            self.asked += 1
            try:
                with urllib.request.urlopen(urllib.request.Request(API, data=body, headers=headers), timeout=120) as answer:
                    said = json.load(answer)
            except urllib.error.HTTPError as error:
                # Too many questions at once: wait for as long as STRATZ says, and ask again.
                if error.code == 429 and attempt < 3:
                    time.sleep(float(error.headers.get("Retry-After") or 20))
                    continue
                sys.exit(f"STRATZ answered {error.code} {error.reason}")
            except (urllib.error.URLError, TimeoutError) as error:
                if attempt < 3:
                    time.sleep(5)
                    continue
                sys.exit(f"STRATZ could not be reached: {error}")
            if said.get("errors"):
                # STRATZ's database is full for a while: wait a minute and ask the same again,
                # up to ten times, rather than lose all that was asked before.
                if any(CAPACITY in str(error.get("message")) for error in said["errors"]) and busy < BUSY_TRIES:
                    busy += 1
                    attempt -= 1
                    time.sleep(BUSY_WAIT)
                    continue
                sys.exit("STRATZ did not like the question: " + "; ".join(str(error.get("message")) for error in said["errors"][:3]))
            # A short rest, to stay far inside the limit of questions a second.
            time.sleep(0.3)
            return said["data"]
        sys.exit("STRATZ kept refusing")


def weeks_to_add_up(weeks, patch_began, now):
    """The weeks to add up, newest first: the whole weeks since the newest patch began, no more
    than MOST_WEEKS, and where those are fewer than LEAST_WEEKS, the newest whole weeks there are."""
    whole = sorted((week for week in set(weeks) if week + WEEK <= now), reverse=True)
    of_the_patch = [week for week in whole if week >= patch_began][:MOST_WEEKS]
    return of_the_patch if len(of_the_patch) >= LEAST_WEEKS else whole[:LEAST_WEEKS]


def expected_against(mine, theirs):
    """How often a hero that wins `mine` of its matches is to win against one that wins `theirs`,
    if the two had nothing to do with each other."""
    win, lose = mine * (1 - theirs), theirs * (1 - mine)
    return win / (win + lose) if win + lose > 0 else 0.5


def expected_with(mine, theirs):
    """The same for two heroes on one team."""
    win, lose = mine * theirs, (1 - mine) * (1 - theirs)
    return win / (win + lose) if win + lose > 0 else 0.5


def said_of_a_pair(matches, wins, expected):
    """What the file says of a pair: by how much it won more often than expected, in tenths of
    a percent, pulled towards nothing the fewer matches there were."""
    if matches <= 0:
        return 0
    more = wins / matches - expected
    believed = more * matches / (matches + DOUBT)
    return max(-MOST, min(MOST, round(believed * 1000)))


def tables(heroes, pairs_against, pairs_with):
    """The two tables of one pair of ranks. `heroes` are the heroes' numbers in the order of
    the file; `pairs_against[(a, b)]` is [matches, wins of a] for a against b, and
    `pairs_with[(a, b)]` the same for a beside b."""
    matches = {hero: 0 for hero in heroes}
    wins = {hero: 0 for hero in heroes}
    for (hero, _), (played, won) in pairs_against.items():
        if hero in matches:
            matches[hero] += played
            wins[hero] += won
    rate = {hero: wins[hero] / matches[hero] if matches[hero] else 0.5 for hero in heroes}

    def table(pairs, expected, mirrored):
        rows = []
        for hero in heroes:
            row = []
            for other in heroes:
                played, won = pairs.get((hero, other), (0, 0))
                row.append(0 if hero == other else said_of_a_pair(played, won, expected(rate[hero], rate[other])))
            rows.append(row)
        # STRATZ counts a pair once from each of its two heroes, and the two counts differ a
        # little. The table says one thing of a pair: the middle of the two, which for two
        # heroes against each other is as much for the one as it is against the other.
        for a in range(len(heroes)):
            for b in range(a + 1, len(heroes)):
                middle = round((rows[a][b] + mirrored * rows[b][a]) / 2)
                rows[a][b], rows[b][a] = middle, mirrored * middle
        return rows

    return table(pairs_against, expected_against, -1), table(pairs_with, expected_with, 1)


def build(matches, starting, boots, bought, talents, is_talent):
    """One build, from what STRATZ counts for a hero in a role: each kind of thing as a list
    of short lists of numbers, the things named by STRATZ's own numbers for them. `matches`
    is how many matches the counts are of. `is_talent` says of a skill's number whether it
    is a talent."""
    start = []
    # For each item, each copy: the matches it was had in. The first copy counts where it was
    # bought. STRATZ calls every copy after it that is bought in the same moment "given", as
    # the second, third and fourth Iron Branch: the purchase logs of such matches have each of
    # them bought, so those count whether they were bought or "given".
    by_item = {}
    for row in starting or []:
        if row["instance"] == 0 and row.get("wasGiven"):
            continue
        held = by_item.setdefault(row["itemId"], {}).setdefault(row["instance"], [0, 0])
        held[0] += row["matchCount"]
        held[1] += row["winCount"]
    for item, copies in by_item.items():
        one = copies.get(0)
        if one and one[0] >= START_SHARE * matches:
            # How many of it are bought: the copies that most of those who buy it have, one
            # after the other.
            count = 1
            while count in copies and copies[count][0] >= COPY_SHARE * one[0]:
                count += 1
            start.append([item, count, one[0], one[1]])
    start.sort(key=lambda entry: -entry[2])

    # Boots that are bought before the match begins have a time before nought: minute 0.
    shoes = [[row["itemId"], max(0, round((row.get("timeAverage") or 0) / 60)), row["matchCount"], row["winCount"]] for row in boots or [] if row["matchCount"] >= BOOTS_SHARE * matches]
    shoes = sorted(shoes, key=lambda entry: -entry[2])[:MOST_BOOTS]
    all_boots = {row["itemId"] for row in boots or []}

    sums = {}
    for row in bought or []:
        if row.get("instance", 0) == 0 and row["itemId"] not in all_boots:
            item = sums.setdefault(row["itemId"], [0, 0, 0])
            item[0] += row["matchCount"]
            item[1] += row["winCount"]
            item[2] += row["time"] * row["matchCount"]
    # What is bought in the first minute is a starting item, which has its own list.
    items = [[item, round(minutes / count), count, wins] for item, (count, wins, minutes) in sums.items() if count >= ITEM_SHARE * matches and minutes / count >= 1]
    # The boots are items among the others, in the order all of them are bought in.
    items = sorted(sorted(items, key=lambda entry: -entry[2])[:MOST_ITEMS] + shoes, key=lambda entry: (entry[1], -entry[2]))

    taken = sorted(([row["abilityId"], row["matchCount"], row["winCount"]] for row in talents or [] if is_talent(row["abilityId"]) and row["matchCount"] > 0), key=lambda entry: -entry[1])
    return {"matches": matches, "start": start[:MOST_START], "items": items, "talents": taken}


def usual_order(orders):
    """The order most players put their skill points in, point by point. `orders` are the
    orders of single matches, each a list of skills. Each point is what most of those do who
    did the same up to there; where few did, it is what most of everybody does with that
    point, as far as the points before it leave it to be had."""
    # No skill is put more points in than the most any one match put in it.
    most_of = collections.Counter()
    for one in orders:
        for skill, points in collections.Counter(one).items():
            most_of[skill] = max(most_of[skill], points)
    order, the_same = [], orders
    while len(order) < MOST_POINTS:
        place = len(order)
        so_far = [one for one in orders if len(one) > place]
        if not so_far or len(so_far) < GOT_SO_FAR * len(orders):
            break
        the_same = [one for one in the_same if len(one) > place]
        asked = the_same if len(the_same) >= LEAST_THE_SAME else so_far
        liked = collections.Counter(one[place] for one in asked).most_common()
        skill = next((skill for skill, _ in liked if order.count(skill) < most_of[skill]), None)
        if skill is None:
            break
        order.append(skill)
        the_same = [one for one in the_same if one[place] == skill]
    return order


def bought_against(samples):
    """Which items are bought more often against which hero. `samples[(hero, role)]` are the
    matches of a hero in a role, each a pair: the five heroes of the other team, and the
    items that were bought. An item is counted against a hero over all heroes that met it:
    how often it was bought in those matches, against how often those who met it buy it in
    all of their matches. Returns, for each enemy hero, a list of [item, times as often],
    the most first."""
    seen, expected = collections.Counter(), collections.Counter()
    for matches in samples.values():
        if len(matches) < LEAST_SAMPLE:
            continue
        usual = collections.Counter(item for _, items in matches for item in items)
        met = collections.Counter(enemy for enemies, _ in matches for enemy in enemies)
        for enemy, times in met.items():
            for item, bought in usual.items():
                expected[(enemy, item)] += times * bought / len(matches)
        for enemies, items in matches:
            for enemy in enemies:
                for item in items:
                    seen[(enemy, item)] += 1
    # Against some heroes matches are longer, and more of everything is bought: that is
    # taken out, so that what is left is what is bought against the hero itself.
    all_seen, all_expected = collections.Counter(), collections.Counter()
    for (enemy, _), bought in seen.items():
        all_seen[enemy] += bought
    for (enemy, _), bought in expected.items():
        all_expected[enemy] += bought
    against = {}
    for (enemy, item), usually in expected.items():
        bought = seen[(enemy, item)]
        if usually < LEAST_EXPECTED or bought < usually + SURE * math.sqrt(usually):
            continue
        more_of_all = all_seen[enemy] / all_expected[enemy] if all_expected[enemy] else 1.0
        times = (bought + DOUBT_BOUGHT) / (usually + DOUBT_BOUGHT) / more_of_all
        if times >= LEAST_TIMES:
            against.setdefault(enemy, []).append([item, round(min(times, MOST_TIMES), 2)])
    return {enemy: sorted(items, key=lambda entry: (-entry[1], entry[0])) for enemy, items in against.items()}


def pro_builds(began, now):
    """What the pros who played each hero in each role bought, in the pro matches since
    `began`: for each hero, by its number, and role, how many pros played it so, and the items
    that at least PRO_SHARE of them bought, each by its name in the game's files, with how many
    bought it and the minute they first bought it in on average. Asks OpenDota once; where
    that does not work, says so and gives nothing.

    A pro match says which lane each player was in (`lane_role`: 1 the safe lane, 2 the mid, 3
    the off lane), and nothing of roles. Of the two in the safe lane, the one with more last
    hits at minute 10 is taken for the carry and the other for the hard support; of the two in
    the off lane, the one with more for the offlaner and the other for the soft support; and
    the one in mid with the most for the mid. A third player in a lane is left out."""
    sql = """
        WITH players AS (
            SELECT pm.match_id, pm.player_slot, pm.player_slot < 128 AS radiant, pm.lane_role, pm.hero_id,
                pm.purchase_log, COALESCE(pm.lh_t[11], 0) AS lh10
            FROM player_matches pm JOIN matches m USING (match_id)
            WHERE m.start_time >= %(began)d AND m.start_time < %(now)d AND pm.lane_role IN (1, 2, 3)
                AND pm.purchase_log IS NOT NULL
        ), placed AS (
            SELECT *, row_number() OVER (PARTITION BY match_id, radiant, lane_role ORDER BY lh10 DESC, player_slot) AS place
            FROM players
        ), roled AS (
            SELECT match_id, player_slot, hero_id, purchase_log,
                CASE WHEN lane_role = 1 AND place = 1 THEN 0 WHEN lane_role = 2 AND place = 1 THEN 1
                    WHEN lane_role = 3 AND place = 1 THEN 2 WHEN lane_role = 3 AND place = 2 THEN 3
                    WHEN lane_role = 1 AND place = 2 THEN 4 END AS role
            FROM placed
        ), counted AS (
            SELECT hero_id, role, count(*) AS players FROM roled WHERE role IS NOT NULL GROUP BY 1, 2
        ), firsts AS (
            SELECT r.hero_id, r.role, r.match_id, r.player_slot, p->>'key' AS item, min((p->>'time')::int) AS time
            FROM roled r, unnest(r.purchase_log) AS p
            WHERE r.role IS NOT NULL
            GROUP BY 1, 2, 3, 4, 5
        )
        SELECT f.hero_id, f.role, c.players, f.item, count(*) AS bought, round(avg(f.time)) AS time,
            (SELECT count(DISTINCT match_id) FROM players) AS matches
        FROM firsts f JOIN counted c USING (hero_id, role)
        WHERE c.players >= %(least)d
        GROUP BY f.hero_id, f.role, c.players, f.item
        HAVING count(*) >= %(share)f * c.players
        ORDER BY f.hero_id, f.role, bought DESC
    """ % {"began": began, "now": now, "least": LEAST_PROS, "share": PRO_SHARE}
    try:
        # OpenDota turns away Python's own name for itself, and takes a plain one.
        asked = urllib.request.Request(PRO_API + urllib.parse.quote(" ".join(sql.split())), headers={"User-Agent": "make-numbers/1.0"})
        with urllib.request.urlopen(asked, timeout=180) as answer:
            said = json.load(answer)
    except (urllib.error.URLError, TimeoutError, ValueError) as error:
        print(f"  OpenDota could not be asked ({error}): the file has nothing of what pros buy")
        return 0, {}
    if said.get("err") or not isinstance(said.get("rows"), list):
        print(f"  OpenDota did not like the question ({said.get('err')}): the file has nothing of what pros buy")
        return 0, {}
    heroes, matches = {}, 0
    for row in said["rows"]:
        matches = max(matches, int(row["matches"] or 0))
        players, items = heroes.setdefault((int(row["hero_id"]), int(row["role"])), (int(row["players"]), []))
        # What is bought before the match begins, on average, is of the start and not of
        # what is bought after it.
        seconds = float(row["time"] or 0)
        if seconds > 0:
            items.append(["item_" + row["item"], int(row["bought"]), round(seconds / 60)])
    return matches, heroes


def sample_of(asker, hero, role):
    """Single matches of a hero in a role, of Divine and Immortal players: for each the
    heroes of the other team, the skills in the order they got their points, and the items
    that were bought."""
    answer = asker.ask(
        "{ heroStats { guide(heroId:%d, positionId:%s, take:%d) { guides(take:%d) { match { players { heroId isRadiant } } "
        "matchPlayer { isRadiant abilities { abilityId } stats { itemPurchases { itemId } } } } } } }" % (hero, ROLES[role], SAMPLE, SAMPLE)
    )
    matches = []
    for listed in answer["heroStats"]["guide"] or []:
        for guide in listed.get("guides") or []:
            match, player = guide.get("match") or {}, guide.get("matchPlayer")
            if not player or not match.get("players"):
                continue
            enemies = [other["heroId"] for other in match["players"] if other["isRadiant"] != player["isRadiant"]]
            skills = [point["abilityId"] for point in player.get("abilities") or []]
            items = {bought["itemId"] for bought in (player.get("stats") or {}).get("itemPurchases") or []}
            matches.append((enemies, skills, items))
    return matches


def roles_played(numbers, pair, hero):
    """The roles a hero is played in often enough in a pair of ranks, the most played first.
    `hero` is its place in the file's list."""
    played = [sum(numbers["ranks"][rank]["played"][role][hero] for rank in (2 * pair, 2 * pair + 1)) for role in range(len(ROLES))]
    often = [role for role in range(len(ROLES)) if played[role] > 0 and played[role] >= PLAYED_SHARE * sum(played)]
    return sorted(often, key=lambda role: -played[role])[:MOST_ROLES]


def make_builds(asker, now, numbers, heroes, few, patch_began):
    """The builds of every hero of the Draft page's numbers. `heroes` are STRATZ's numbers for
    the heroes, in the order of that file's list. `few`: for this many heroes only.
    `patch_began`: when the patch began, for the pro matches of it."""
    constants = asker.ask("{ constants { items { id name } abilities { id name isTalent } } }")["constants"]
    item_names = {item["id"]: item["name"] for item in constants["items"] if item.get("name")}
    skill_names = {skill["id"]: skill["name"] for skill in constants["abilities"] if skill.get("name")}
    talents = {skill["id"] for skill in constants["abilities"] if skill.get("isTalent") or (skill.get("name") or "").startswith("special_bonus")}
    # The points a hero puts in its attributes are counted among the talents, and are none.
    skill_names = {skill: name for skill, name in skill_names.items() if name != "special_bonus_attributes"}

    of_hero = {}
    # Every item STRATZ counts among what is bought after the start, however few buy it: the
    # items that are not used up, and that are not the parts of something else alone.
    counted_items = set()
    for place, hero in list(enumerate(heroes))[: few or len(heroes)]:
        builds = []
        for pair, (ranks, _) in enumerate(PAIRS):
            roles = roles_played(numbers, pair, place)
            if not roles:
                continue
            asked = "heroId:%d, bracketBasicIds:[%s], positionIds:[%%s]" % (hero, ranks)
            parts = " ".join(
                (
                    "s%(r)d: itemStartingPurchase(%(of)s) { itemId instance wasGiven matchCount winCount } "
                    "b%(r)d: itemBootPurchase(%(of)s) { itemId timeAverage matchCount winCount } "
                    # STRATZ counts what is bought after the start by the minute, and leaves
                    # out every minute that fewer than about 250 matches bought an item in,
                    # unless `matchLimit` says otherwise. An item bought at many different
                    # minutes, as most that are bought for the match at hand, is then not
                    # there at all: Puck's Aghanim's Scepter, bought in 24 of 100 Divine and
                    # Immortal matches, was missing.
                    "i%(r)d: itemFullPurchase(%(of)s, matchLimit:0) { itemId instance time matchCount winCount } "
                    "t%(r)d: talent(%(of)s) { abilityId matchCount winCount } "
                    "a%(r)d: abilityMinLevel(%(of)s) { abilityId level matchCount }"
                )
                % {"r": role, "of": asked % ROLES[role]}
                for role in roles
            )
            answer = asker.ask("{ heroStats { %s } }" % parts)["heroStats"]
            for role in roles:
                counted_items.update(row["itemId"] for row in answer["i%d" % role] or [])
                first = answer["a%d" % role] or []
                # Every match has a skill at level 1: those are the matches the counts are of.
                matches = sum(row["matchCount"] for row in first if row["level"] == 1 and row["abilityId"] not in talents)
                if matches < LEAST_MATCHES:
                    continue
                bought = [[row for row in answer[kind % role] or [] if item_names.get(row["itemId"]) not in LOST_BY_STRATZ] for kind in ("s%d", "b%d", "i%d")]
                made = build(matches, *bought, answer["t%d" % role], talents.__contains__)
                builds.append({"role": role, "pair": pair, **made})
        if builds:
            of_hero[hero] = builds
        if (place + 1) % 20 == 0:
            print(f"  builds of {place + 1} heroes, {asker.asked} questions asked")

    # The items and skills the builds name, each once, by the names the game's files know them
    # by. A build names them by their place in these lists. What STRATZ has no name for is
    # left out of the builds.
    items, skills = [], []

    def place_in(names, known, number):
        name = known.get(number)
        if name is None:
            return None
        if name not in names:
            names.append(name)
        return names.index(name)

    def named(entries, names, known):
        placed = [[place_in(names, known, entry[0])] + entry[1:] for entry in entries]
        return [entry for entry in placed if entry[0] is not None]

    for builds in of_hero.values():
        for made in builds:
            for kind in ("start", "items"):
                made[kind] = named(made[kind], items, item_names)
            made["talents"] = named(made["talents"], skills, skill_names)

    # Single matches of each hero in each role it has a build for: the order of its skill
    # points, and what is bought against whom.
    samples, orders, sampled = {}, {}, 0
    for count, (hero, builds) in enumerate(of_hero.items()):
        for role in sorted({made["role"] for made in builds}):
            matches = sample_of(asker, hero, role)
            sampled += len(matches)
            samples[(hero, role)] = [(enemies, bought) for enemies, _, bought in matches]
            if len(matches) < LEAST_SAMPLE:
                continue
            order = []
            for skill in usual_order([points for _, points, _ in matches]):
                place = place_in(skills, skill_names, skill)
                # A point in something STRATZ has no name for: the order ends before it.
                if place is None:
                    break
                order.append(place)
            if order:
                orders.setdefault(hero, []).append([role, len(matches), order])
        if (count + 1) % 20 == 0:
            print(f"  single matches of {count + 1} heroes, {sampled:,} in all, {asker.asked} questions asked")
    # Only what some build has among its items is said to be bought against a hero.
    in_a_build = set(items)
    against = {}
    for enemy, bought in bought_against(samples).items():
        said = [[items.index(item_names[item]), round(times * 100)] for item, times in bought if item_names.get(item) in in_a_build][:MOST_AGAINST]
        if said and enemy in heroes:
            against[numbers["heroes"][heroes.index(enemy)]] = said

    def name_of(hero):
        return numbers["heroes"][heroes.index(hero)]

    # What pros buy in each role, of the items STRATZ counts: a Town Portal Scroll that
    # everybody buys is not to be told.
    pro_matches, pros = pro_builds(patch_began, now)
    countable = {item_names[item] for item in counted_items if item in item_names}
    pro_heroes = {}
    for (hero, role), (players, bought) in sorted(pros.items()):
        said = [[place_of(items, name), count, minute] for name, count, minute in bought if name in countable]
        if said and hero in of_hero:
            pro_heroes.setdefault(name_of(hero), []).append([role, players, said])
    print(f"  what pros buy: {sum(len(roles) for roles in pro_heroes.values())} roles of {len(pro_heroes)} heroes, from {pro_matches:,} pro matches")

    return {
        "format": 2,
        "made": now,
        "source": "STRATZ",
        "patch": numbers["patch"],
        "sampled": sampled,
        "items": items,
        "skills": skills,
        "heroes": {name_of(hero): builds for hero, builds in of_hero.items()},
        "orders": {name_of(hero): of_it for hero, of_it in orders.items()},
        "against": against,
        "pros": {"source": "OpenDota", "matches": pro_matches, "heroes": pro_heroes},
    }


def place_of(names, name):
    """The place of a name in a list of names, which gets it where it does not have it."""
    if name not in names:
        names.append(name)
    return names.index(name)


def make(asker, now):
    """The numbers of the Draft page, STRATZ's numbers for the heroes in the order of its list,
    and when the newest patch began."""
    start = asker.ask("{ constants { heroes { id name displayName } gameVersions { id name asOfDateTime } } heroStats { winWeek(heroIds:[1]) { week } } }")
    names = {hero["id"]: hero["name"] for hero in start["constants"]["heroes"] if hero.get("name")}
    patch = max(start["constants"]["gameVersions"], key=lambda version: version["id"])
    weeks = weeks_to_add_up([row["week"] for row in start["heroStats"]["winWeek"]], patch["asOfDateTime"], now)
    if not weeks:
        sys.exit("STRATZ has no whole week of matches to add up")

    ranks, pairs = [], []
    heroes = None
    for pair, its_ranks in PAIRS:
        # Hero against hero and hero beside hero, a week at a time, added up.
        pairs_against, pairs_with = {}, {}
        for week in weeks:
            answer = asker.ask(
                "{ heroStats { matchUp(bracketBasicIds:[%s], week:%d, take:500) { heroId "
                "vs { heroId2 matchCount winCount } with { heroId2 matchCount winCount } } } }" % (pair, week)
            )
            for of_hero in answer["heroStats"]["matchUp"]:
                hero = of_hero["heroId"]
                for sums_of, rows in ((pairs_against, of_hero["vs"]), (pairs_with, of_hero["with"])):
                    for row in rows or []:
                        sums = sums_of.setdefault((hero, row["heroId2"]), [0, 0])
                        sums[0] += row["matchCount"]
                        sums[1] += row["winCount"]
        if heroes is None:
            # The heroes of the file: those STRATZ has a name and matches for, by their numbers.
            heroes = sorted({hero for hero, _ in pairs_against if hero in names})
        against, beside = tables(heroes, pairs_against, pairs_with)
        pairs.append({"against": against, "with": beside})
        print(f"{pair}: {sum(sums[0] for sums in pairs_against.values()) // 5:,} matches of a hero against the heroes of the other team")

        # Matches and wins in each role, in ranked matches, added up over the same weeks: for
        # each of the pair's two ranks by itself.
        for rank in its_ranks:
            parts = " ".join(
                "r%d: winWeek(bracketIds:[%s], positionIds:[%s], gameModeIds:[ALL_PICK_RANKED]) { week heroId matchCount winCount }" % (place, rank, role)
                for place, role in enumerate(ROLES)
            )
            answer = asker.ask("{ heroStats { %s } }" % parts)["heroStats"]
            played, won = [], []
            for place in range(len(ROLES)):
                sums = {hero: [0, 0] for hero in heroes}
                for row in answer["r%d" % place] or []:
                    if row["week"] in weeks and row["heroId"] in sums:
                        sums[row["heroId"]][0] += row["matchCount"]
                        sums[row["heroId"]][1] += row["winCount"]
                played.append([sums[hero][0] for hero in heroes])
                won.append([sums[hero][1] for hero in heroes])
            ranks.append({"played": played, "won": won})
            print(f"  {rank}: {sum(map(sum, played)):,} ranked matches of a hero with a role")

    if heroes is None or len(heroes) < LEAST_HEROES:
        sys.exit("STRATZ named too few heroes: nothing was written")
    lanes = [lanes_of_a_pair(asker, pair, weeks, heroes) for pair, _ in PAIRS]
    numbers = {
        "format": 1,
        "made": now,
        "source": "STRATZ",
        "patch": patch["name"],
        "weeks": len(weeks),
        "heroes": [names[hero] for hero in heroes],
        "ranks": ranks,
        "pairs": pairs,
        "lanes": lanes,
    }
    return numbers, heroes, patch["asOfDateTime"]


def lane_sums(row):
    """[meetings, won, drawn] of a row of STRATZ's lane outcomes. STRATZ counts a lane that was
    won by far (a stomp) apart from one that was won, and the same for lost."""
    won = row["winCount"] + row["stompWinCount"]
    drawn = row["drawCount"]
    lost = row["lossCount"] + row["stompLossCount"]
    return [won + drawn + lost, won, drawn]


def lane_rate(sums):
    """The share of lanes won, a draw counting half."""
    meetings, won, drawn = sums
    return (won + drawn / 2) / meetings if meetings else 0.5


def lane_table(heroes, meetings):
    """What a lane table of one role says. `meetings[(a, b)]` is [meetings, won, drawn] of a in
    that role against (or beside) b in its lane. Returns each hero's lanes added up, and for
    each two heroes by how much more often the first came out of its lanes ahead with the second
    there than expected, in tenths of a percent, pulled towards nothing the fewer times they met.

    Expected is the first hero's own share of lanes won, plus what the second does to the lanes
    of that role as a rule: the share by which the heroes that met it did worse or better than
    in all their lanes, each by how often it met them. So a hero that every carry loses its lane
    against is not a reason against one carry more than another, and a pair that met seldom is
    pulled towards that rule, not towards an even lane. A lane is not the same from both of its
    sides, so nothing is mirrored."""
    own = {hero: [0, 0, 0] for hero in heroes}
    for (hero, _), sums in meetings.items():
        if hero in own:
            own[hero] = [mine + more for mine, more in zip(own[hero], sums)]
    base = {hero: lane_rate(own[hero]) for hero in heroes}
    # What each hero does to the lanes of the heroes that met it, weighted by the meetings.
    effect_sums = {hero: [0.0, 0] for hero in heroes}
    for (hero, other), sums in meetings.items():
        if hero in own and other in effect_sums and hero != other and sums[0]:
            effect_sums[other][0] += sums[0] * (lane_rate(sums) - base[hero])
            effect_sums[other][1] += sums[0]
    effect = {hero: total / count if count else 0.0 for hero, (total, count) in effect_sums.items()}
    rows = []
    for hero in heroes:
        row = []
        for other in heroes:
            sums = meetings.get((hero, other))
            if hero == other or not sums or not sums[0]:
                row.append(0)
                continue
            expected = min(1.0, max(0.0, base[hero] + effect[other]))
            row.append(said_of_a_pair(sums[0], sums[1] + sums[2] / 2, expected))
        rows.append(row)
    return own, rows


# The most rows STRATZ gives in one answer about lanes. An answer that long may have been cut.
LANE_ROWS_CUT = 16000


def lanes_of_a_pair(asker, pair, weeks, heroes):
    """How lanes go in a pair of ranks, for each of the five roles: each hero's own lanes, and
    each hero against the heroes it met in its lane and beside the one it shared its lane with."""
    against = [{} for _ in ROLES]
    beside = [{} for _ in ROLES]
    fields = "heroId1 heroId2 matchCount winCount stompWinCount drawCount lossCount stompLossCount"
    for week in weeks:
        for is_with, sums_of in ((False, against), (True, beside)):
            parts = " ".join(
                "r%d: laneOutcome(isWith:%s, week:%d, bracketBasicIds:[%s], positionIds:[%s]) { %s }" % (place, "true" if is_with else "false", week, pair, role, fields)
                for place, role in enumerate(ROLES)
            )
            answer = asker.ask("{ heroStats { %s } }" % parts)["heroStats"]
            # The role is the one that was asked for: STRATZ's answer names another.
            for place in range(len(ROLES)):
                rows = answer["r%d" % place] or []
                if len(rows) >= LANE_ROWS_CUT:
                    # Perhaps cut short: that role is asked for again, a hero at a time.
                    print(f"  {len(rows)} rows for {ROLES[place]} in {pair}: asked again hero by hero")
                    rows = []
                    for hero in heroes:
                        one = "laneOutcome(heroId:%d, isWith:%s, week:%d, bracketBasicIds:[%s], positionIds:[%s]) { %s }" % (hero, "true" if is_with else "false", week, pair, ROLES[place], fields)
                        rows += [row for row in asker.ask("{ heroStats { %s } }" % one)["heroStats"]["laneOutcome"] or [] if row["heroId1"] == hero]
                for row in rows:
                    key = (row["heroId1"], row["heroId2"])
                    sums_of[place][key] = [mine + more for mine, more in zip(sums_of[place].get(key, [0, 0, 0]), lane_sums(row))]
    tables = {"played": [], "won": [], "drawn": [], "against": [], "with": []}
    for place in range(len(ROLES)):
        own, rows_against = lane_table(heroes, against[place])
        _, rows_beside = lane_table(heroes, beside[place])
        for key, column in (("played", 0), ("won", 1), ("drawn", 2)):
            tables[key].append([own[hero][column] for hero in heroes])
        tables["against"].append(rows_against)
        tables["with"].append(rows_beside)
    won = [lane_rate([sum(tables[key][place]) for key in ("played", "won", "drawn")]) for place in range(len(ROLES))]
    print(f"  lanes of {pair}, won by role: " + ", ".join(f"{share:.1%}" for share in won))
    return tables


def check():
    """Tries the sums on made-up numbers."""
    assert abs(expected_against(0.5, 0.5) - 0.5) < 1e-9
    assert expected_against(0.55, 0.45) > 0.55, "a strong hero is to win against a weak one more often than against the average"
    assert abs(expected_against(0.52, 0.47) + expected_against(0.47, 0.52) - 1) < 1e-9
    assert abs(expected_with(0.5, 0.5) - 0.5) < 1e-9 and expected_with(0.55, 0.55) > 0.55

    assert said_of_a_pair(0, 0, 0.5) == 0
    assert said_of_a_pair(400, 220, 0.5) == 25, "5% more, believed by half"
    assert said_of_a_pair(40000, 22000, 0.5) == 50, "many matches are believed nearly in full"
    assert said_of_a_pair(10, 10, 0.5) == 12, "ten wins out of ten are luck"
    assert said_of_a_pair(10**6, 10**6, 0.1) == MOST and said_of_a_pair(10**6, 0, 0.9) == -MOST

    # Three heroes that each win half of their matches: the first beats the second, the second
    # the third and the third the first, six times out of ten.
    beats = {(1, 2): [4000, 2400], (2, 3): [4000, 2400], (3, 1): [4000, 2400], (2, 1): [4000, 1600], (3, 2): [4000, 1600], (1, 3): [4000, 1600]}
    even = {pair: [4000, 2000] for pair in beats}
    against, beside = tables([1, 2, 3], beats, even)
    assert against[0][1] == 91 and against[1][0] == -91 and against[0][2] == -91, "10% more, less the doubt, in tenths of a percent"
    assert against[0][0] == 0 and beside == [[0, 0, 0]] * 3

    day = 24 * 3600
    # Lanes: a stomp is a win, a draw is half of one, and a lane is not mirrored.
    assert lane_sums({"winCount": 3, "stompWinCount": 2, "drawCount": 4, "lossCount": 1, "stompLossCount": 1}) == [11, 5, 4]
    assert lane_rate([10, 4, 2]) == 0.5 and lane_rate([0, 0, 0]) == 0.5
    # Heroes 1 and 2 each win half of their lanes. 1 does well against 3 and badly against 4,
    # and 2 the other way round. Both lose 7 of 10 against 5, which is what 5 does to everybody.
    meetings = {
        (1, 3): [1000, 600, 0], (1, 4): [1000, 400, 0], (2, 3): [1000, 400, 0], (2, 4): [1000, 600, 0],
        (1, 5): [1000, 300, 0], (2, 5): [1000, 300, 0], (3, 1): [500, 250, 0],
    }
    own, rows = lane_table([1, 2, 3, 4, 5], meetings)
    assert own[1] == [3000, 1300, 0] and own[5] == [0, 0, 0]
    assert rows[0][2] == 71 and rows[0][3] == -71 and rows[1][2] == -71 and rows[1][3] == 71, "10% either way, believed by 1000 of 1400"
    assert rows[0][4] == 0 and rows[1][4] == 0, "losing to 5 as everybody does is nothing against one hero"
    assert rows[2][0] == 0, "the other side of a lane says what it says, not the opposite"
    assert rows[4] == [0] * 5 and rows[0][0] == 0
    # A pair that met seldom is pulled towards what the other hero does as a rule.
    _, rows = lane_table([1, 2, 5], {(1, 5): [10000, 3000, 0], (1, 2): [10000, 5000, 0], (2, 5): [40, 16, 0], (2, 1): [10000, 5000, 0]})
    assert -5 < rows[1][2] < 5, "2 did as badly against 5 as 1 does: no reason against it"

    weeks = [10 * WEEK, 9 * WEEK, 8 * WEEK, 7 * WEEK, 6 * WEEK, 5 * WEEK]
    now = 11 * WEEK + day
    assert weeks_to_add_up(weeks, 0, now) == weeks[:4], "no more than four"
    assert weeks_to_add_up(weeks, 8 * WEEK, now) == weeks[:3], "the weeks since the patch"
    assert weeks_to_add_up(weeks, 10 * WEEK - day, now) == weeks[:2], "two at the least, though one is before the patch"
    assert weeks_to_add_up(weeks, 0, 10 * WEEK + day)[0] == 9 * WEEK, "a week that is not over is left out"

    # A build of 1000 matches. Item 1 is bought twice at the start by most, item 2 by few.
    starting = [
        {"itemId": 1, "instance": 0, "wasGiven": False, "matchCount": 900, "winCount": 500},
        {"itemId": 1, "instance": 1, "wasGiven": False, "matchCount": 700, "winCount": 400},
        {"itemId": 1, "instance": 2, "wasGiven": False, "matchCount": 100, "winCount": 50},
        {"itemId": 1, "instance": 2, "wasGiven": True, "matchCount": 400, "winCount": 210},
        {"itemId": 1, "instance": 3, "wasGiven": True, "matchCount": 300, "winCount": 160},
        {"itemId": 2, "instance": 0, "wasGiven": False, "matchCount": 100, "winCount": 60},
        {"itemId": 3, "instance": 0, "wasGiven": True, "matchCount": 1000, "winCount": 500},
    ]
    boots = [{"itemId": 5, "timeAverage": 610.0, "matchCount": 800, "winCount": 420}, {"itemId": 6, "timeAverage": 2000.0, "matchCount": 20, "winCount": 15}, {"itemId": 11, "timeAverage": -75.0, "matchCount": 100, "winCount": 50}]
    bought = [
        {"itemId": 7, "instance": 0, "time": 14, "matchCount": 300, "winCount": 170},
        {"itemId": 7, "instance": 0, "time": 16, "matchCount": 300, "winCount": 150},
        {"itemId": 7, "instance": 1, "time": 30, "matchCount": 500, "winCount": 250},
        {"itemId": 8, "instance": 0, "time": 0, "matchCount": 900, "winCount": 450},
        {"itemId": 9, "instance": 0, "time": 30, "matchCount": 20, "winCount": 15},
        {"itemId": 5, "instance": 0, "time": 10, "matchCount": 800, "winCount": 420},
        {"itemId": 10, "instance": 0, "time": 8, "matchCount": 500, "winCount": 240},
    ]
    talents = [{"abilityId": 90, "matchCount": 600, "winCount": 330}, {"abilityId": 91, "matchCount": 350, "winCount": 160}, {"abilityId": 20, "matchCount": 5, "winCount": 5}]
    made = build(1000, starting, boots, bought, talents, lambda skill: skill >= 90)
    assert made["start"] == [[1, 3, 900, 500]], "three of the first item, the third bought or 'given' in 500 of the 900 matches it is in, and not a fourth; what few buy, and a first copy that is given, are left out"
    assert made["items"] == [[11, 0, 100, 50], [10, 8, 500, 240], [5, 10, 800, 420], [7, 15, 600, 320]], "in the order they are bought, boots among them, each with its minute and never one before the match; without starting items, second copies and what few buy"
    assert made["talents"] == [[90, 600, 330], [91, 350, 160]], "talents alone, the most taken first"
    assert sorted(made) == ["items", "matches", "start", "talents"]

    # Skill points: most put the first in skill 1, then two in skill 2. Of the few that
    # begin with skill 2, most go on with skill 3, but that is not what the order says.
    orders = [[1, 2, 2, 9]] * 12 + [[1, 2, 1]] * 6 + [[2, 3, 3, 3]] * 5 + [[1]] * 2
    assert usual_order(orders) == [1, 2, 2, 9], "what most do who did the same so far; the last point is one that enough got to"
    assert usual_order([[1, 2]] * 2 + [[1]] * 10) == [1], "a point that few matches got as far as is not said"
    # Where few did the same so far, everybody is asked, and a skill that has all its points
    # is not given another.
    assert usual_order([[1, 1, 2], [1, 1, 2], [2, 1, 1], [2, 1, 1], [1, 2, 1]]) == [1, 1, 2]
    assert usual_order([]) == []

    # Items against heroes. Heroes in a role, 400 matches each. Hero 7 is met in a quarter
    # of them, and item 50 is bought in half of those and in a tenth of the others.
    def matches_of(often, seldom):
        with_it, without = (7, 2, 3, 4, 5), (6, 2, 3, 4, 5)
        return [(with_it, {50} if place < often else {60}) for place in range(100)] + [(without, {50} if place < seldom else {60}) for place in range(300)]

    said = bought_against({(1, 0): matches_of(50, 30), (8, 2): matches_of(50, 30), (9, 4): matches_of(50, 30)[:20]})
    assert list(said) == [7], "against the hero it is bought against, and no other"
    assert [item for item, _ in said[7]] == [50] and 2.0 < said[7][0][1] < 2.2, "50 in 100 against 20 in 100 as usual, less the doubt"
    assert bought_against({(1, 0): matches_of(25, 75)}) == {}, "bought as often as ever: nothing is said"
    assert bought_against({(1, 0): matches_of(3, 0)[:120]}) == {}, "three purchases are luck"

    def played(carry, mid, soft):
        return {"played": [[carry], [mid], [0], [soft], [0]]}

    numbers = {"ranks": [played(600, 0, 0), played(300, 80, 20), played(0, 0, 0), played(0, 0, 0)]}
    assert roles_played(numbers, 0, 0) == [0], "the carry, and not the mid of 8 matches in 100"
    assert roles_played(numbers, 1, 0) == [], "no matches, no roles"
    print("The sums are right.")


def write(folder, name, what):
    """Writes one of the two files, and says how large it is."""
    text = json.dumps(what, separators=(",", ":")) + "\n"
    if len(text.encode("utf-8")) > LIMITS[name]:
        sys.exit(f"{name} would have {len(text):,} bytes, more than the app takes: nothing was written")
    path = os.path.join(folder, name)
    with open(path, "w", encoding="utf-8", newline="\n") as file:
        file.write(text)
    return f"{path}: {len(text):,} bytes"


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    if sys.argv[1] == "--check":
        return check()
    folder = sys.argv[1]
    os.makedirs(folder, exist_ok=True)
    asker = Asker(token())
    now = int(time.time())
    numbers, heroes, patch_began = make(asker, now)
    print(write(folder, "draft.json", numbers) + f", {len(numbers['heroes'])} heroes, patch {numbers['patch']}, {numbers['weeks']} weeks, {asker.asked} questions asked")
    few = int(os.environ.get("DME_FEW_HEROES") or 0)
    builds = make_builds(asker, now, numbers, heroes, few, patch_began)
    count = sum(len(of_hero) for of_hero in builds["heroes"].values())
    print(write(folder, "builds.json", builds) + f", {count} builds of {len(builds['heroes'])} heroes, {builds['sampled']:,} single matches, {asker.asked} questions asked in all")


if __name__ == "__main__":
    main()
