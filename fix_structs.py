import os
import re

def fix_file(path):
    with open(path, 'r') as f:
        content = f.read()

    lines = content.split('\n')
    new_lines = []
    for line in lines:
        if 'tags: vec![' in line and 'constellation_density:' not in line:
            line = line.replace('tags:', 'constellation_density: None, tags:')
        elif 'tags: Vec::new()' in line and 'constellation_density:' not in line:
            line = line.replace('tags:', 'constellation_density: None, tags:')
        elif 'high_band_ratio:' in line and 'constellation_density:' not in line:
             if 'sf.' in line or 'fs.' in line:
                 var = 'sf' if 'sf.' in line else 'fs'
                 line = line.replace('high_band_ratio:', f'constellation_density: {var}.constellation_density as f32, high_band_ratio:')
                 # wait, for SpectralFeatures (database.rs) it should be f64
                 if 'SpectralFeatures' in content and 'database.rs' in path:
                      line = line.replace('as f32', 'as f64')
             else:
                 line = line.replace('high_band_ratio:', 'constellation_density: 0.0, high_band_ratio:')

        # Handle Segment { ... } multi-line
        if 'kind:' in line and 'SegmentKind' in line and 'constellation_density' not in line:
            # check next few lines for constellation_density
            pass # this is getting complicated, let's just do more simple replacements

        new_lines.append(line)

    new_content = '\n'.join(new_lines)
    if new_content != content:
        with open(path, 'w') as f:
            f.write(new_content)

for root, dirs, files in os.walk('src'):
    for file in files:
        if file.endswith('.rs'):
            fix_file(os.path.join(root, file))

for root, dirs, files in os.walk('tests'):
    for file in files:
        if file.endswith('.rs'):
            fix_file(os.path.join(root, file))
