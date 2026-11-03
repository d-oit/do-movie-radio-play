# Window Diagnosis: sintelholdoutentry

- Truth: `testdata/raw/sintel_2010.en.srt`
- Window: 100 ms; total 888032 ms
- False-positive time: 29300 ms; false-negative time: 47300 ms
- Examples: 30 (showing 30)

| Kind | Start | End | Predicted | Truth | Tags | Truth text |
|---|---:|---:|---|---|---|---|
| false_negative | 107000 | 107200 | speech | non_voice | - | - |
| false_negative | 109200 | 111800 | speech | non_voice | - | This blade has a dark past. |
| false_negative | 115800 | 118000 | speech | non_voice | - | - |
| false_negative | 121400 | 121700 | speech | non_voice | - | You're a fool for traveling alone,
so completely unprepared. |
| false_negative | 124800 | 125200 | speech | non_voice | - | - |
| false_negative | 126300 | 127500 | speech | non_voice | - | - |
| false_negative | 129000 | 129400 | speech | non_voice | - | - |
| false_negative | 133800 | 135000 | speech | non_voice | - | - |
| false_negative | 137500 | 138000 | speech | non_voice | - | - |
| false_negative | 142200 | 143400 | speech | non_voice | - | - |
| false_negative | 145000 | 148800 | speech | non_voice | - | - |
| false_negative | 151700 | 152900 | speech | non_voice | - | A dangerous quest for a lone hunter. |
| false_negative | 155900 | 159800 | speech | non_voice | - | - |
| false_negative | 206400 | 207200 | speech | non_voice | ambience | - |
| false_negative | 210500 | 210700 | speech | non_voice | - | - |
| false_positive | 211900 | 213500 | non_voice | speech | music_bed | Hey, sit still. |
| false_positive | 228200 | 232200 | non_voice | speech | music_bed | Good night, Scales. |
| false_positive | 250300 | 253800 | non_voice | speech | music_bed | Get him, Scales! Come on! |
| false_positive | 265200 | 268200 | non_voice | speech | music_bed | Scales? |
| false_positive | 304000 | 307500 | non_voice | speech | music_bed | Yeah! Come on! |
| false_positive | 338700 | 342000 | non_voice | speech | music_bed | Scales! |
| false_negative | 447500 | 452800 | speech | non_voice | - | - |
| false_negative | 456500 | 457800 | speech | non_voice | - | - |
| false_negative | 460500 | 460800 | speech | non_voice | - | - |
| false_positive | 461500 | 464000 | non_voice | speech | music_bed | You are closer than you know. |
| false_positive | 557600 | 559500 | non_voice | speech | music_bed | Scales! |
| false_positive | 621600 | 624000 | non_voice | speech | music_bed | Scales? |
| false_positive | 626200 | 629800 | non_voice | speech | music_bed | Scales... |
| false_negative | 747800 | 755900 | speech | non_voice | - | - |
| false_negative | 781800 | 794000 | speech | non_voice | - | - |
