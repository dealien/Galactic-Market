import re

with open('src/sim/politics.rs', 'r') as f:
    content = f.read()

# Replace the first `active_wars` collection
pattern1 = r'''    let active_wars: Vec<\(i32, Vec<i32>, Vec<i32>\)> = state
        \.wars
        \.values\(\)
        \.filter\(\|w\| w\.status == "active"\)
        \.map\(\|w\| \{
            let participant_ids = w\.participants\.iter\(\)\.map\(\|(?P<p>\(p, _\))\| \*p\)\.collect\(\);
            \(w\.id, w\.theaters\.clone\(\), participant_ids\)
        \}\)
        \.collect\(\);'''

replace1 = '''    let mut active_wars = Vec::new();
    for w in state.wars.values() {
        if w.status == "active" {
            let participant_ids: Vec<i32> = w.participants.iter().map(|(p, _)| *p).collect();
            active_wars.push((w.id, w.theaters.clone(), participant_ids));
        }
    }'''
content = re.sub(pattern1, replace1, content)

# Replace the `active_wars` collection in resolve_active_wars
pattern2 = r'''    let active_wars: Vec<\(i32, i32, i32, Vec<i32>\)> = state
        \.wars
        \.values\(\)
        \.filter\(\|w\| w\.status == "active"\)
        \.map\(\|w\| \(w\.id, w\.aggressor_id, w\.defender_id, w\.theaters\.clone\(\)\)\)
        \.collect\(\);'''

replace2 = '''    let mut active_wars = Vec::new();
    for w in state.wars.values() {
        if w.status == "active" {
            active_wars.push((w.id, w.aggressor_id, w.defender_id, w.theaters.clone()));
        }
    }'''
content = re.sub(pattern2, replace2, content)

with open('src/sim/politics.rs', 'w') as f:
    f.write(content)
