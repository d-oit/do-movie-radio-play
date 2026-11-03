# Window Diagnosis: tos-de-holdout

- Truth: `testdata/raw/tears_of_steel_2012.de.srt`
- Window: 100 ms; total 734167 ms
- False-positive time: 47600 ms; false-negative time: 66367 ms
- Examples: 76 (showing 50)

| Kind | Start | End | Predicted | Truth | Tags | Truth text |
|---|---:|---:|---|---|---|---|
| false_negative | 0 | 9700 | speech | non_voice | - | - |
| false_positive | 23000 | 24500 | non_voice | speech | impact_heavy | Du bist ein Vollidiot, Thom. |
| false_negative | 24700 | 25000 | speech | non_voice | - | - |
| false_negative | 30500 | 30800 | speech | non_voice | - | - |
| false_negative | 34000 | 34500 | speech | non_voice | - | - |
| false_negative | 36000 | 37000 | speech | non_voice | - | - |
| false_negative | 42000 | 42200 | speech | non_voice | - | - |
| false_negative | 45000 | 50000 | speech | non_voice | - | - |
| false_negative | 53500 | 61000 | speech | non_voice | - | - |
| false_positive | 62200 | 63000 | non_voice | speech | impact_heavy,music_bed,music_like | Das ist ziemlich irre. |
| false_positive | 117000 | 119000 | non_voice | speech | impact_heavy,music_bed,music_like | Solltest du nicht da unten sein? |
| false_positive | 131000 | 131600 | non_voice | speech | impact_heavy,music_bed,music_like | Ich habe euch gestern Nacht reden hören. |
| false_negative | 133000 | 136000 | speech | non_voice | - | - |
| false_positive | 137400 | 138000 | non_voice | speech | impact_heavy,music_bed,music_like | Es ist nicht meine Schuld, weißt du. |
| false_positive | 191000 | 192000 | non_voice | speech | impact_heavy,music_bed,music_like | Bist du bereit? |
| false_negative | 195000 | 197000 | speech | non_voice | - | - |
| false_negative | 198000 | 199000 | speech | non_voice | - | - |
| false_negative | 201000 | 201500 | speech | non_voice | - | - |
| false_negative | 203000 | 203500 | speech | non_voice | - | - |
| false_negative | 206500 | 208000 | speech | non_voice | - | - |
| false_negative | 212000 | 213000 | speech | non_voice | - | - |
| false_negative | 217000 | 223000 | speech | non_voice | - | - |
| false_negative | 224000 | 224500 | speech | non_voice | - | - |
| false_negative | 225700 | 226000 | speech | non_voice | - | - |
| false_positive | 226000 | 227000 | non_voice | speech | music_bed,music_like | Thom! |
| false_positive | 231000 | 233000 | non_voice | speech | music_bed,music_like | Da ist sie. |
| false_positive | 233500 | 234500 | non_voice | speech | music_bed,music_like | Also: |
| false_positive | 235000 | 236000 | non_voice | speech | music_bed,music_like | Du liebst sie. |
| false_positive | 237000 | 237700 | non_voice | speech | music_bed,music_like | Sie ist dein Ein und Alles. |
| false_negative | 238500 | 240000 | speech | non_voice | - | - |
| false_negative | 241000 | 241500 | speech | non_voice | - | - |
| false_negative | 244500 | 245000 | speech | non_voice | - | - |
| false_positive | 247600 | 248000 | non_voice | speech | impact_heavy,music_bed,music_like | Erinnere sie daran was Liebe ist. |
| false_positive | 292000 | 295000 | non_voice | speech | impact_heavy,music_bed,music_like | ...und, Action! |
| false_positive | 296500 | 297500 | non_voice | speech | impact_heavy,music_bed,music_like | Überschreiben von Speicher läuft! |
| false_negative | 298000 | 299000 | speech | non_voice | - | - |
| false_negative | 301000 | 302500 | speech | non_voice | - | - |
| false_positive | 304200 | 305000 | non_voice | speech | music_bed,music_like | Oh - Entschuldigung. |
| false_positive | 316000 | 316400 | non_voice | speech | music_bed,music_like | Schau Celia. |
| false_negative | 317000 | 318000 | speech | non_voice | - | - |
| false_negative | 320000 | 321000 | speech | non_voice | - | - |
| false_negative | 323000 | 325000 | speech | non_voice | - | - |
| false_positive | 327800 | 328000 | non_voice | speech | impact_heavy,music_bed,music_like | ...und ich will einfach nur superflott durch das Weltall düsen. |
| false_positive | 339000 | 339600 | non_voice | speech | impact_heavy,music_bed,music_like | Okay, sie kommen. Noch zwei Minuten! |
| false_negative | 341000 | 343000 | speech | non_voice | - | - |
| false_negative | 344000 | 345000 | speech | non_voice | - | - |
| false_negative | 346000 | 347000 | speech | non_voice | - | - |
| false_negative | 350000 | 356000 | speech | non_voice | - | - |
| false_negative | 359000 | 359500 | speech | non_voice | - | - |
| false_negative | 363000 | 365000 | speech | non_voice | - | - |
