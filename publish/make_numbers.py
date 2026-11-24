"""Makes the two files of numbers the app's Draft and Builds pages work from.

draft.json: how well each hero does in each of the five roles, for each of the eight ranks,
how well it does against and beside each other hero, for each pair of ranks, and how its lanes
go in each role, against the heroes it meets there and beside the one it shares them with.
And the newest week and the week before it, each by itself, to say which heroes are rising
and which falling.

builds.json: what the players of each hero buy and skill, in each role the hero is played in,
for each pair of ranks: the items they start with, the items they go on to buy and when,
boots among them, and which talents are taken. And from single matches of Divine and
Immortal players, some hundreds for each hero and role: the order the skill points are put
in, point by point, and which items are bought more often against which enemy hero. And from
OpenDota (https://www.opendota.com), the pro matches of the patch, and of the four weeks
before now where the patch is newer than that: what the pros who play a hero in each role
buy, such as the Bottle that nearly every pro mid buys. And from OpenDota's pro matches of the
newest patch alone: which neutral items, trinkets and enhancements, pros take in each tier.

    python publish/make_numbers.py <folder to write the two files in>
    python publish/make_numbers.py --check        # tries the sums below on made-up numbers

The numbers are STRATZ's (https://stratz.com), asked for through its API with a token that a
Steam login there gives. The token is read from the environment variable STRATZ_TOKEN or, where
that is not set, from the file .dme/stratz.key in the home folder. It goes nowhere but to
STRATZ, and never into the file that is written. OpenDota is asked without a token, two
questions in all. Where it cannot be reached, the file is written without what pros buy and
without neutral items.

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
# OpenDota's database of pro matches, which takes a question in SQL, and its ability id map.
PRO_API = "https://api.opendota.com/api/explorer?sql="
OPENDOTA_ABILITY_IDS = "https://api.opendota.com/api/constants/ability_ids"

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

# A file of builds with less than this is not written, and nothing at all is: STRATZ gave
# hollow answers (on 2026-11-22, in the middle of a patch, the totals of a hero in a role came
# back with a few matches or none, and single matches without their player), and last week's
# files are better than one whose builds are mostly missing. The file of 2026-11-18 had 127
# heroes, 1,056 builds of 299 heroes in a role, a skill order for 276 of those, and items
# bought against 88 heroes: these are about two thirds of that, or less.
LEAST_BUILDS = 700
LEAST_HERO_ROLES = 200
LEAST_ORDERS = 150
LEAST_AGAINST = 40
# A build that has this part of its players' talents, and its items, is not hollow.
LEAST_WITH_TALENTS = 0.8
LEAST_ITEMS = 10

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
# The pro matches that are counted are those since the newest patch began, and those of at
# least this many days: in the first weeks of a patch too few of its own are played to go by
# (26 in the first two days of 7.42, where the nine weeks of 7.41f had 1,350), and what the
# pros bought until then, such as a mid's Bottle, would be gone from every build.
PRO_LEAST_DAYS = 28

# Items whose purchases STRATZ loses, so that its numbers of them are not to be gone by. Of
# five ranked matches whose mid player bought a Bottle in the second minute by OpenDota's
# records, and in four of them still had it at the end, STRATZ had a Bottle in none: 5 of 100
# Storm Spirits by its numbers, where pros, and ranked players by OpenDota's records, buy one
# nearly always. What pros buy (OpenDota) is what says whether to have it.
LOST_BY_STRATZ = {"item_bottle"}

# Neutral items: what pros take in each tier, from OpenDota's pro matches of the newest patch
# alone, since a patch changes the trinkets of each tier (7.42 brought four new ones and made
# Revenant's Brooch a trinket). A hero in a role is said where at least LEAST_NEUTRAL_PROS pros
# played it so; where fewer did, the hero in any role, where at least as many played it. A tier
# is said where at least LEAST_NEUTRAL_TIER of them got as far, and a trinket or enhancement
# where at least NEUTRAL_SHARE of those took it, and two at the least: one pro is nobody to go
# by. No more than MOST_TRINKETS and MOST_ENHANCEMENTS of each tier. The newest patch's pro
# matches are counted over no more than NEUTRAL_MOST_DAYS, which only bounds the question.
LEAST_NEUTRAL_PROS = 5
LEAST_NEUTRAL_TIER = 3
NEUTRAL_SHARE = 0.15
MOST_TRINKETS, MOST_ENHANCEMENTS = 3, 2
NEUTRAL_TIERS = 4
NEUTRAL_MOST_DAYS = 180
# Pro fallback for heroes whose skill order or talents STRATZ lacks (such as Kez and Largo).
PRO_SKILLS_DAYS = 90
LEAST_PRO_SKILLS = 8
# The role of a hero's neutral items that are of every role it was played in.
ANY_ROLE = 5
# What OpenDota calls a neutral item where the game's files call it otherwise.
OPENDOTA_NEUTRALS = {"guardian_shell": "defiant_shell"}


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


def patches_of(versions, weeks):
    """The name of the patch the weeks are of, or of the patches where they are of more than
    one: "7.41f and 7.42". For the first two weeks of a patch, the weeks added up are of the
    one before it (`weeks_to_add_up`), and the file says so rather than the newest patch's
    name. `versions` are STRATZ's, with `name` and `asOfDateTime`."""
    by_date = sorted(versions, key=lambda version: version["asOfDateTime"])
    names = []
    for version in by_date:
        later = [other["asOfDateTime"] for other in by_date if other["asOfDateTime"] > version["asOfDateTime"]]
        ended = min(later) if later else float("inf")
        if version["asOfDateTime"] < max(weeks) + WEEK and ended > min(weeks):
            names.append(version["name"])
    return " and ".join(names) if len(names) <= 2 else f"{names[0]} to {names[-1]}"


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


def pros_since(patch_began, now):
    """When the pro matches that are counted begin: with the newest patch, or PRO_LEAST_DAYS
    ago where the patch is newer than that."""
    return min(patch_began, now - PRO_LEAST_DAYS * 24 * 3600)


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


def neutral_rows(now):
    """What pros took in each tier of neutral items, in OpenDota's pro matches of the newest
    patch: the rows of one question, which `neutrals_of` makes sense of. Where OpenDota cannot
    be asked, says so and gives none.

    The newest patch is that of OpenDota's newest pro match (its table `match_patch`, which
    names each match's patch by when it was played: "7.42", without the letters of the small
    patches after it). STRATZ names a patch only some days after it came out.

    A player's `neutral_item_history` has what was made at each tier, in order: the first is
    of tier 1, the second of tier 2, and so on, each with the trinket that was taken and its
    enhancement. One without a trinket kept the one before and took only a new enhancement.
    What comes after the fourth is made again at tier 4, and is not counted. The roles are told
    apart as for what pros buy (`pro_builds`)."""
    sql = """
        WITH newest AS (
            SELECT mp.patch FROM match_patch mp
            WHERE mp.match_id = (SELECT match_id FROM matches WHERE start_time < %(now)d ORDER BY start_time DESC LIMIT 1)
        ), games AS (
            SELECT m.match_id FROM matches m JOIN match_patch mp USING (match_id) JOIN newest n ON mp.patch = n.patch
            WHERE m.start_time >= %(since)d AND m.start_time < %(now)d
        ), players AS (
            SELECT pm.match_id, pm.player_slot, pm.player_slot < 128 AS radiant, pm.lane_role, pm.hero_id,
                pm.neutral_item_history AS history, COALESCE(pm.lh_t[11], 0) AS lh10
            FROM player_matches pm JOIN games USING (match_id)
            WHERE pm.neutral_item_history IS NOT NULL
        ), placed AS (
            SELECT *, row_number() OVER (PARTITION BY match_id, radiant, lane_role ORDER BY lh10 DESC, player_slot) AS place
            FROM players
        ), roled AS (
            SELECT hero_id, history,
                CASE WHEN lane_role = 1 AND place = 1 THEN 0 WHEN lane_role = 2 AND place = 1 THEN 1
                    WHEN lane_role = 3 AND place = 1 THEN 2 WHEN lane_role = 3 AND place = 2 THEN 3
                    WHEN lane_role = 1 AND place = 2 THEN 4 END AS role
            FROM placed
        ), made AS (
            SELECT r.hero_id, r.role, h.tier, COALESCE(h.made->>'item_neutral', '') AS trinket,
                COALESCE(h.made->>'item_neutral_enhancement', '') AS enhancement,
                CASE WHEN (h.made->>'time') ~ '^[0-9]+$' THEN (h.made->>'time')::int END AS made_time
            FROM roled r, unnest(r.history) WITH ORDINALITY AS h(made, tier)
            WHERE h.tier <= %(tiers)d
        )
        SELECT 'm' AS kind, 0 AS hero_id, 0 AS role, 0 AS tier, (SELECT patch FROM newest) AS name, count(*) AS count FROM games
        UNION ALL
        SELECT 'a', 0, 0, tier, '', COALESCE(round(avg(made_time) / 60.0)::int, 0)
        FROM made WHERE made_time IS NOT NULL GROUP BY tier
        UNION ALL
        SELECT 'p', hero_id, CASE WHEN GROUPING(role) = 1 THEN %(any)d ELSE role END, 0, '', count(*)
        FROM roled GROUP BY GROUPING SETS ((hero_id, role), (hero_id)) HAVING GROUPING(role) = 1 OR role IS NOT NULL
        UNION ALL
        SELECT 't', hero_id, CASE WHEN GROUPING(role) = 1 THEN %(any)d ELSE role END, tier, trinket, count(*)
        FROM made GROUP BY GROUPING SETS ((hero_id, role, tier, trinket), (hero_id, tier, trinket)) HAVING GROUPING(role) = 1 OR role IS NOT NULL
        UNION ALL
        SELECT 'e', hero_id, CASE WHEN GROUPING(role) = 1 THEN %(any)d ELSE role END, tier, enhancement, count(*)
        FROM made WHERE enhancement <> '' GROUP BY GROUPING SETS ((hero_id, role, tier, enhancement), (hero_id, tier, enhancement)) HAVING GROUPING(role) = 1 OR role IS NOT NULL
    """ % {"now": now, "since": now - NEUTRAL_MOST_DAYS * 24 * 3600, "tiers": NEUTRAL_TIERS, "any": ANY_ROLE}
    try:
        asked = urllib.request.Request(PRO_API + urllib.parse.quote(" ".join(sql.split())), headers={"User-Agent": "make-numbers/1.0"})
        with urllib.request.urlopen(asked, timeout=180) as answer:
            said = json.load(answer)
    except (urllib.error.URLError, TimeoutError, ValueError) as error:
        print(f"  OpenDota could not be asked ({error}): the file has no neutral items")
        return []
    if said.get("err") or not isinstance(said.get("rows"), list):
        print(f"  OpenDota did not like the question ({said.get('err')}): the file has no neutral items")
        return []
    return said["rows"]


def neutrals_of(rows):
    """The pros' neutral items, made of the rows of `neutral_rows`: the patch, how many pro
    matches, average craft minutes per tier, the names of the trinkets and enhancements, by the
    names the game's files know them by, and for each hero, by its number, the roles enough pros
    played it in, each with how many and its tiers. ANY_ROLE stands for every role together,
    which is there where some role of the hero is not. A tier is its number, how many got as
    far, how many of those kept the trinket they had, and the trinkets and the enhancements
    taken, each by its place in the names and how many took it, the most taken first."""
    patch, matches = "", 0
    minutes = [0] * NEUTRAL_TIERS
    players, reached, taken = {}, collections.Counter(), {}
    for row in rows:
        kind, hero, role, tier, name, count = row["kind"], int(row["hero_id"]), int(row["role"]), int(row["tier"]), row["name"] or "", int(row["count"])
        if kind == "m":
            patch, matches = name, count
        elif kind == "a" and 1 <= tier <= NEUTRAL_TIERS:
            minutes[tier - 1] = max(0, count)
        elif kind == "p":
            players[(hero, role)] = count
        elif kind == "t" and not name:
            # Kept the trinket of the tier before, and took only a new enhancement.
            reached[(hero, role, tier)] += count
            taken.setdefault((hero, role, tier), {"kept": 0, "t": [], "e": []})["kept"] += count
        elif kind in ("t", "e") and name:
            if kind == "t":
                reached[(hero, role, tier)] += count
            name = "item_" + OPENDOTA_NEUTRALS.get(name, name)
            taken.setdefault((hero, role, tier), {"kept": 0, "t": [], "e": []})[kind].append((name, count))
    names, heroes = [], {}
    for hero in sorted({hero for hero, _ in players}):
        roles = [role for role in range(ANY_ROLE) if players.get((hero, role), 0) >= LEAST_NEUTRAL_PROS]
        # The hero in any role, for the roles too few pros played it in for their own.
        if len(roles) < ANY_ROLE and players.get((hero, ANY_ROLE), 0) >= LEAST_NEUTRAL_PROS:
            roles.append(ANY_ROLE)
        for role in roles:
            tiers = []
            for tier in range(1, NEUTRAL_TIERS + 1):
                got = reached[(hero, role, tier)]
                if got < LEAST_NEUTRAL_TIER:
                    continue
                of_tier = taken.get((hero, role, tier), {"kept": 0, "t": [], "e": []})

                def most(entries, at_most):
                    enough = [(name, count) for name, count in entries if count >= 2 and count >= NEUTRAL_SHARE * got]
                    return [[place_of(names, name), count] for name, count in sorted(enough, key=lambda entry: (-entry[1], entry[0]))[:at_most]]

                tiers.append([tier, got, of_tier["kept"], most(of_tier["t"], MOST_TRINKETS), most(of_tier["e"], MOST_ENHANCEMENTS)])
            if tiers:
                heroes.setdefault(hero, []).append([role, players[(hero, role)], tiers])
    out = {"source": "OpenDota", "patch": patch, "matches": matches, "names": names, "heroes": heroes}
    if any(minutes):
        out["minutes"] = minutes
    return out


def pro_skills_from_rows(rows, ability_names):
    """Computes pro skill orders and talent pick/win counts per (hero, role) and per (hero, ANY_ROLE)
    from OpenDota rows: each row has hero_id, role (or None), win (bool), and upgrades (list of
    ability IDs). Used to fill in heroes whose skill orders or talents STRATZ lacks (such as Kez
    and Largo)."""
    orders_by = collections.defaultdict(list)
    talents_by = collections.defaultdict(lambda: collections.defaultdict(lambda: [0, 0]))
    for row in rows:
        hero = int(row["hero_id"])
        role = row.get("role")
        role = int(role) if role is not None else None
        won = 1 if row.get("win") else 0
        upgrades = [ability_names.get(str(aid)) for aid in (row.get("upgrades") or [])]
        upgrades = [name for name in upgrades if name and name != "special_bonus_attributes"]
        if not upgrades:
            continue
        keys = [(hero, ANY_ROLE)] + ([(hero, role)] if role is not None else [])
        for key in keys:
            orders_by[key].append(upgrades)
            for name in set(upgrades):
                if name.startswith("special_bonus"):
                    counts = talents_by[key][name]
                    counts[0] += 1
                    counts[1] += won
    orders = {}
    for key, lists in orders_by.items():
        if len(lists) >= LEAST_PRO_SKILLS:
            order = usual_order(lists)
            if order:
                orders[key] = (len(lists), order)
    talents = {}
    for key, counts in talents_by.items():
        if len(orders_by[key]) >= LEAST_PRO_SKILLS:
            taken = sorted(([name, m, w] for name, (m, w) in counts.items() if m > 0), key=lambda entry: (-entry[1], entry[0]))
            if taken:
                talents[key] = taken
    return orders, talents


def pro_skills_and_talents(now, hero_ids):
    """Fetches pro skill orders and talent counts from OpenDota for `hero_ids` (heroes whose
    skill orders or talents STRATZ lacks, such as Kez and Largo). Returns ({(hero, role): (n, [names])},
    {(hero, role): [[talent_name, matches, wins]]}). Fails soft to empty dicts."""
    if not hero_ids:
        return {}, {}
    try:
        req = urllib.request.Request(OPENDOTA_ABILITY_IDS, headers={"User-Agent": "make-numbers/1.0"})
        with urllib.request.urlopen(req, timeout=60) as answer:
            ability_names = json.load(answer)
    except (urllib.error.URLError, TimeoutError, ValueError) as error:
        print(f"  OpenDota ability map could not be fetched ({error}): no pro skill fallback")
        return {}, {}
    id_list = ",".join(str(int(h)) for h in sorted(set(hero_ids)))
    sql = """
        WITH players AS (
            SELECT pm.match_id, pm.player_slot, pm.player_slot < 128 AS radiant, pm.lane_role, pm.hero_id,
                (pm.player_slot < 128) = m.radiant_win AS win,
                pm.ability_upgrades_arr AS upgrades, COALESCE(pm.lh_t[11], 0) AS lh10
            FROM player_matches pm JOIN matches m USING (match_id)
            WHERE m.start_time >= %(since)d AND m.start_time < %(now)d
                AND pm.hero_id IN (%(heroes)s) AND pm.ability_upgrades_arr IS NOT NULL
        ), placed AS (
            SELECT *, row_number() OVER (PARTITION BY match_id, radiant, lane_role ORDER BY lh10 DESC, player_slot) AS place
            FROM players
        )
        SELECT hero_id, win, upgrades,
            CASE WHEN lane_role = 1 AND place = 1 THEN 0 WHEN lane_role = 2 AND place = 1 THEN 1
                WHEN lane_role = 3 AND place = 1 THEN 2 WHEN lane_role = 3 AND place = 2 THEN 3
                WHEN lane_role = 1 AND place = 2 THEN 4 END AS role
        FROM placed
    """ % {"since": now - PRO_SKILLS_DAYS * 24 * 3600, "now": now, "heroes": id_list}
    try:
        asked = urllib.request.Request(PRO_API + urllib.parse.quote(" ".join(sql.split())), headers={"User-Agent": "make-numbers/1.0"})
        with urllib.request.urlopen(asked, timeout=180) as answer:
            said = json.load(answer)
    except (urllib.error.URLError, TimeoutError, ValueError) as error:
        print(f"  OpenDota pro skills could not be asked ({error}): no pro skill fallback")
        return {}, {}
    if said.get("err") or not isinstance(said.get("rows"), list):
        print(f"  OpenDota did not like the pro skills question ({said.get('err')})")
        return {}, {}
    return pro_skills_from_rows(said["rows"], ability_names)


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


def make_builds(asker, now, numbers, heroes, few, patch):
    """The builds of every hero of the Draft page's numbers. `heroes` are STRATZ's numbers for
    the heroes, in the order of that file's list. `few`: for this many heroes only.
    `patch`: STRATZ's newest, with its name and when it began, for the pro matches since then
    (`pros_since`)."""
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

    # Pro fallback for heroes whose skill orders or talents STRATZ lacks (such as Kez and Largo).
    missing_skill_heroes = sorted({
        hero for hero, builds in of_hero.items()
        if any(not made["talents"] for made in builds)
        or any(not any(entry[0] == role for entry in orders.get(hero, [])) for role in {made["role"] for made in builds})
    })
    if missing_skill_heroes:
        pro_orders, pro_talents = pro_skills_and_talents(now, missing_skill_heroes)
        added_orders, added_talents = 0, 0
        for hero in missing_skill_heroes:
            builds = of_hero[hero]
            have_order = {entry[0] for entry in orders.get(hero, [])}
            for role in sorted({made["role"] for made in builds}):
                if role not in have_order:
                    got = pro_orders.get((hero, role)) or pro_orders.get((hero, ANY_ROLE))
                    if got:
                        count_pro, seq = got
                        orders.setdefault(hero, []).append([role, count_pro, [place_of(skills, name) for name in seq]])
                        added_orders += 1
            for made in builds:
                if not made["talents"]:
                    got = pro_talents.get((hero, made["role"])) or pro_talents.get((hero, ANY_ROLE))
                    if got:
                        made["talents"] = [[place_of(skills, name), m, w] for name, m, w in got]
                        added_talents += 1
        if added_orders or added_talents:
            print(f"  pro skill/talent fallback: {added_orders} skill orders, {added_talents} builds with talents")
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
    pro_matches, pros = pro_builds(pros_since(patch["asOfDateTime"], now), now)
    countable = {item_names[item] for item in counted_items if item in item_names}
    pro_heroes = {}
    for (hero, role), (players, bought) in sorted(pros.items()):
        said = [[place_of(items, name), count, minute] for name, count, minute in bought if name in countable]
        if said and hero in of_hero:
            pro_heroes.setdefault(name_of(hero), []).append([role, players, said])
    print(f"  what pros buy: {sum(len(roles) for roles in pro_heroes.values())} roles of {len(pro_heroes)} heroes, from {pro_matches:,} pro matches")

    # The neutral items pros take, of the newest patch alone, for the heroes the file knows.
    neutrals = neutrals_of(neutral_rows(now))
    neutrals["heroes"] = {name_of(hero): of_it for hero, of_it in neutrals["heroes"].items() if hero in heroes}
    print(f"  neutral items: {sum(len(roles) for roles in neutrals['heroes'].values())} roles of {len(neutrals['heroes'])} heroes, from {neutrals['matches']:,} pro matches of {neutrals['patch'] or 'no patch'}")

    return {
        "format": 2,
        "made": now,
        "source": "STRATZ",
        # STRATZ counts builds over the newest patch, whatever weeks the Draft page's
        # numbers add up.
        "patch": patch["name"],
        "sampled": sampled,
        "items": items,
        "skills": skills,
        "heroes": {name_of(hero): builds for hero, builds in of_hero.items()},
        "orders": {name_of(hero): of_it for hero, of_it in orders.items()},
        "against": against,
        "pros": {"source": "OpenDota", "matches": pro_matches, "heroes": pro_heroes},
        # A key the apps of 0.4.3 and before do not know, and pass over: the file stays of format 2.
        "neutrals": neutrals,
    }


def hollow(builds):
    """What a file of builds lacks to be written, in a sentence, or None where it lacks
    nothing (LEAST_BUILDS and the numbers after it)."""
    made = [made for of_hero in builds["heroes"].values() for made in of_hero]
    full = [made for made in made if made["talents"] and len(made["items"]) >= LEAST_ITEMS]
    hero_roles = {(hero, made["role"]) for hero, of_hero in builds["heroes"].items() for made in of_hero}
    orders = sum(len(of_hero) for of_hero in builds["orders"].values())
    missing = []
    if len(builds["heroes"]) < LEAST_HEROES:
        missing.append(f"builds of {len(builds['heroes'])} heroes, where at least {LEAST_HEROES} are wanted")
    if len(made) < LEAST_BUILDS:
        missing.append(f"{len(made)} builds, where at least {LEAST_BUILDS} are wanted")
    if len(hero_roles) < LEAST_HERO_ROLES:
        missing.append(f"builds of {len(hero_roles)} heroes in a role, where at least {LEAST_HERO_ROLES} are wanted")
    if len(full) < LEAST_WITH_TALENTS * len(made):
        missing.append(f"talents and {LEAST_ITEMS} items in {len(full)} of {len(made)} builds")
    if orders < LEAST_ORDERS:
        missing.append(f"a skill order for {orders} heroes in a role, where at least {LEAST_ORDERS} are wanted")
    if len(builds["against"]) < LEAST_AGAINST:
        missing.append(f"items bought against {len(builds['against'])} heroes, where at least {LEAST_AGAINST} are wanted")
    return "; ".join(missing) or None


def place_of(names, name):
    """The place of a name in a list of names, which gets it where it does not have it."""
    if name not in names:
        names.append(name)
    return names.index(name)


def make(asker, now):
    """The numbers of the Draft page, STRATZ's numbers for the heroes in the order of its list,
    and STRATZ's newest patch."""
    start = asker.ask("{ constants { heroes { id name displayName } gameVersions { id name asOfDateTime } } heroStats { winWeek(heroIds:[1]) { week } } }")
    names = {hero["id"]: hero["name"] for hero in start["constants"]["heroes"] if hero.get("name")}
    patch = max(start["constants"]["gameVersions"], key=lambda version: version["id"])
    weeks = weeks_to_add_up([row["week"] for row in start["heroStats"]["winWeek"]], patch["asOfDateTime"], now)
    if not weeks:
        sys.exit("STRATZ has no whole week of matches to add up")

    ranks, pairs, weekly = [], [], []
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
            rows = [answer["r%d" % place] or [] for place in range(len(ROLES))]
            ranks.append(by_role(rows, heroes, weeks))
            print(f"  {rank}: {sum(map(sum, ranks[-1]['played'])):,} ranked matches of a hero with a role")
            # The same answer has every week by itself: the newest of those added up and the
            # week before it are kept apart, to say which heroes are rising and which falling.
            weekly.append([by_role(rows, heroes, [week]) for week in (weeks[0], weeks[0] - WEEK)])

    if heroes is None or len(heroes) < LEAST_HEROES:
        sys.exit("STRATZ named too few heroes: nothing was written")
    lanes = [lanes_of_a_pair(asker, pair, weeks, heroes) for pair, _ in PAIRS]
    numbers = {
        "format": 1,
        "made": now,
        "source": "STRATZ",
        "patch": patches_of(start["constants"]["gameVersions"], weeks),
        "weeks": len(weeks),
        "heroes": [names[hero] for hero in heroes],
        "ranks": ranks,
        "pairs": pairs,
        "lanes": lanes,
        "weekly": {"weeks": [weeks[0], weeks[0] - WEEK], "ranks": weekly},
    }
    return numbers, heroes, patch


def by_role(rows, heroes, weeks):
    """Each hero's matches and wins in each role, added up over `weeks`, from STRATZ's rows of
    one rank: a list of rows for each role, each row a hero's matches and wins in one week."""
    played, won = [], []
    for of_role in rows:
        sums = {hero: [0, 0] for hero in heroes}
        for row in of_role:
            if row["week"] in weeks and row["heroId"] in sums:
                sums[row["heroId"]][0] += row["matchCount"]
                sums[row["heroId"]][1] += row["winCount"]
        played.append([sums[hero][0] for hero in heroes])
        won.append([sums[hero][1] for hero in heroes])
    return {"played": played, "won": won}


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
    versions = [{"name": "7.42", "asOfDateTime": 10 * WEEK + 5 * day}, {"name": "7.41f", "asOfDateTime": 3 * WEEK}, {"name": "7.41e", "asOfDateTime": WEEK}]
    assert patches_of(versions, [9 * WEEK, 8 * WEEK]) == "7.41f", "the weeks before a patch are of the one before it"
    assert patches_of(versions, [10 * WEEK, 9 * WEEK]) == "7.41f and 7.42", "a week a patch came in is of both"
    assert patches_of(versions, [11 * WEEK]) == "7.42"
    assert patches_of(versions, [10 * WEEK, 2 * WEEK]) == "7.41e to 7.42"
    # Matches by role: added up over the weeks asked for, and a week by itself the same way.
    # Hero 9 is not in the file; hero 2 was not played in the week before.
    rows = [
        [{"week": 10 * WEEK, "heroId": 1, "matchCount": 100, "winCount": 60}, {"week": 9 * WEEK, "heroId": 1, "matchCount": 80, "winCount": 40},
         {"week": 8 * WEEK, "heroId": 1, "matchCount": 50, "winCount": 25}, {"week": 10 * WEEK, "heroId": 9, "matchCount": 70, "winCount": 30}],
        [{"week": 10 * WEEK, "heroId": 2, "matchCount": 30, "winCount": 10}],
    ]
    assert by_role(rows, [1, 2], [10 * WEEK, 9 * WEEK]) == {"played": [[180, 0], [0, 30]], "won": [[100, 0], [0, 10]]}
    assert by_role(rows, [1, 2], [10 * WEEK]) == {"played": [[100, 0], [0, 30]], "won": [[60, 0], [0, 10]]}, "the newest week by itself"
    assert by_role(rows, [1, 2], [9 * WEEK]) == {"played": [[80, 0], [0, 0]], "won": [[40, 0], [0, 0]]}, "the week before by itself"
    assert by_role([[], []], [1, 2], [10 * WEEK]) == {"played": [[0, 0], [0, 0]], "won": [[0, 0], [0, 0]]}

    # Pro matches: those of the patch, and in its first weeks those of the weeks before it too.
    assert pros_since(3 * WEEK, now) == 3 * WEEK, "a patch that is old enough: its own matches"
    assert pros_since(now - 2 * day, now) == now - PRO_LEAST_DAYS * day, "a patch of two days: the last four weeks"

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

    # A file of builds as last week's: 125 heroes, each with builds in three roles for two
    # pairs of ranks, skill orders for two of the roles, items bought against 80 heroes.
    def builds_of(heroes=125, roles=3, pairs=2, orders=2, against=80, talents=True, items=20):
        made = {"matches": 5000, "start": [[0, 1, 4000, 2000]], "items": [[1, 10, 3000, 1500]] * items, "talents": [[0, 3000, 1500]] if talents else []}
        return {
            "heroes": {f"h{hero}": [{"role": role, "pair": pair, **made} for role in range(roles) for pair in range(pairs)] for hero in range(heroes)},
            "orders": {f"h{hero}": [[role, 500, [0, 1, 2]] for role in range(orders)] for hero in range(heroes)},
            "against": {f"h{hero}": [[1, 150]] for hero in range(against)},
        }

    assert hollow(builds_of()) is None, "a file as last week's is written"
    few_heroes = hollow(builds_of(roles=1, pairs=2, heroes=30))
    assert "of 30 heroes," in few_heroes and "60 builds," in few_heroes and "of 30 heroes in a role" in few_heroes, "builds of a few heroes alone"
    assert "125 heroes in a role" in hollow(builds_of(roles=1, pairs=4)), "every hero in one role alone"
    assert "skill order for 0" in hollow(builds_of(orders=0)), "single matches without their player give no orders"
    assert "against 0 heroes" in hollow(builds_of(against=0)), "nor what is bought against whom"
    assert "talents and 10 items in 0 of 750" in hollow(builds_of(talents=False)), "builds without their talents"
    assert "in 0 of 750" in hollow(builds_of(items=3)), "builds with three items"

    # Neutral items. Hero 1: 10 pros as carry and 3 as mid. All 13 made tier 1, 6 of the
    # carries tier 2, and 2 tier 3, which is too few to say.
    def row(kind, hero, role, tier, name, count):
        return {"kind": kind, "hero_id": hero, "role": role, "tier": tier, "name": name, "count": str(count)}

    rows = [
        row("m", 0, 0, 0, "7.42", 40), row("a", 0, 0, 1, "", 9), row("a", 0, 0, 2, "", 18),
        row("p", 1, 0, 0, "", 10), row("p", 1, 1, 0, "", 3), row("p", 1, ANY_ROLE, 0, "", 13), row("p", 2, 4, 0, "", 4), row("p", 2, ANY_ROLE, 0, "", 4),
    ]
    rows += [row("t", 1, 0, 1, "pogo_stick", 7), row("t", 1, 0, 1, "guardian_shell", 2), row("t", 1, 0, 1, "chipped_vest", 1), row("e", 1, 0, 1, "enhancement_alert", 9), row("e", 1, 0, 1, "enhancement_vital", 1)]
    rows += [row("t", 1, 0, 2, "brick", 4), row("t", 1, 0, 2, "", 2), row("e", 1, 0, 2, "enhancement_alert", 6), row("t", 1, 0, 3, "balmhook", 2)]
    rows += [row("t", 1, ANY_ROLE, 1, "pogo_stick", 10), row("t", 1, ANY_ROLE, 1, "guardian_shell", 3), row("e", 1, ANY_ROLE, 1, "enhancement_alert", 13)]
    made = neutrals_of(rows)
    assert (made["patch"], made["matches"], made["source"]) == ("7.42", 40, "OpenDota")
    assert made["minutes"] == [9, 18, 0, 0], "pro average craft minutes per tier"
    assert list(made["heroes"]) == [1], "hero 2: too few pros in any role"
    carry, any_role = made["heroes"][1]
    names = made["names"]
    assert carry[:2] == [0, 10] and any_role[:2] == [ANY_ROLE, 13], "the carry's own, and every role for the mid of 3 pros"
    tier_1, tier_2 = carry[2]
    assert [names[place] for place, _ in tier_1[3]] == ["item_pogo_stick", "item_defiant_shell"], "the game's own names, the most taken first; one pro is nobody to go by"
    assert tier_1[:3] == [1, 10, 0] and [names[place] for place, _ in tier_1[4]] == ["item_enhancement_alert"]
    assert tier_2[:3] == [2, 6, 2] and tier_2[3] == [[names.index("item_brick"), 4]], "2 of the 6 kept the trinket they had"
    assert len(carry[2]) == 2, "tier 3: 2 pros got as far, too few"
    empty = neutrals_of([])
    assert empty["heroes"] == {} and empty["matches"] == 0 and empty["names"] == [] and "minutes" not in empty, "nothing asked, nothing said"

    # Pro skill order and talent fallback (such as Kez and Largo).
    ab = {"1": "kez_echo_slash", "2": "kez_kazurai_katana", "3": "special_bonus_unique_kez_1", "4": "special_bonus_attributes"}
    pro_rows = [{"hero_id": 145, "role": 0, "win": i % 2 == 0, "upgrades": [1, 2, 1, 3, 4]} for i in range(10)]
    pro_ord, pro_tal = pro_skills_from_rows(pro_rows, ab)
    assert pro_ord[(145, 0)] == (10, ["kez_echo_slash", "kez_kazurai_katana", "kez_echo_slash", "special_bonus_unique_kez_1"])
    assert pro_tal[(145, 0)] == [["special_bonus_unique_kez_1", 10, 5]], "attributes excluded, wins counted"
    assert pro_ord[(145, ANY_ROLE)][0] == 10
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
    numbers, heroes, patch = make(asker, now)
    print(write(folder, "draft.json", numbers) + f", {len(numbers['heroes'])} heroes, patch {numbers['patch']}, {numbers['weeks']} weeks, {asker.asked} questions asked")
    few = int(os.environ.get("DME_FEW_HEROES") or 0)
    builds = make_builds(asker, now, numbers, heroes, few, patch)
    count = sum(len(of_hero) for of_hero in builds["heroes"].values())
    lacks = hollow(builds)
    if lacks and not few:
        # draft.json goes too: a run that ends here publishes neither (publish/server/update_numbers.sh),
        # and draft.json alone would take builds.json off the branch the apps read.
        os.remove(os.path.join(folder, "draft.json"))
        sys.exit(f"STRATZ's builds are hollow ({lacks}): nothing was written")
    print(write(folder, "builds.json", builds) + f", {count} builds of {len(builds['heroes'])} heroes, {builds['sampled']:,} single matches, {asker.asked} questions asked in all")


if __name__ == "__main__":
    main()
