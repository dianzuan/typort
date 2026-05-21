// Test: multi-level bullet symbols match Word defaults
// Pandoc #4321: custom bullet symbols ignored, need proper hierarchy
// Pain point: level 0=filled circle, level 1=hollow circle, level 2=filled square

- Level 0: filled circle
  - Level 1: hollow circle
    - Level 2: filled square
      - Level 3
  - Another level 1
- Back to level 0

+ First
  + Sub-first
    + Sub-sub-first
  + Sub-second
+ Second
