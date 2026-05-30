// Regression: World::today() must return the real current date, not a fixed
// placeholder. A document using datetime.today() should reflect the day it was
// converted. The test compares against the system date at test time, so it
// stays correct on any day (unlike asserting a literal date).
#set page(width: 8cm, height: 4cm)

Generated on #datetime.today().display("[year]-[month]-[day]").
