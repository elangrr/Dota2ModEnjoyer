"""Makes the file of numbers the app's Draft page works from.

draft.json: how well each hero does in each of the five roles, for each of the eight ranks,
how well it does against and beside each other hero, for each pair of ranks, and how its lanes
go in each role, against the heroes it meets there and beside the one it shares them with.

    python publish/make_numbers.py <folder to write the file in>
    python publish/make_numbers.py --check        # tries the sums below on made-up numbers

The numbers are STRATZ's (https://stratz.com), asked for through its API with a token that a
Steam login there gives. The token is read from the environment variable STRATZ_TOKEN or, where
that is not set, from the file .dme/stratz.key in the home folder. It goes nowhere but to
STRATZ, and never into the file that is written.

About sixty questions are asked, each for all heroes at once, which takes a few minutes.
The file is described in docs/SPEC.md, section 5; the app refuses one that is not of that
shape.
"""

import json
import os
import sys
import time
import urllib.error
import urllib.request

API = "https://api.stratz.com/graphql"

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
# The most bytes the app takes (NUMBERS_LIMIT in crates/dme-core/src/draft.rs). A larger file
# is not written: every app that is out would refuse it.
LIMIT = 4 * 1024 * 1024


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
        for attempt in range(4):
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


def make(asker, now):
    """The numbers of the Draft page."""
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
    return numbers


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

    print("The sums are right.")


def write(folder, name, what):
    """Writes a file, and says how large it is."""
    text = json.dumps(what, separators=(",", ":")) + "\n"
    if len(text.encode("utf-8")) > LIMIT:
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
    numbers = make(asker, now)
    print(write(folder, "draft.json", numbers) + f", {len(numbers['heroes'])} heroes, patch {numbers['patch']}, {numbers['weeks']} weeks, {asker.asked} questions asked")


if __name__ == "__main__":
    main()
