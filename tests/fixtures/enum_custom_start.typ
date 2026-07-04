// An #enum with an explicit start must keep its numbering in Word: the
// second list below renders 4., 5. in Typst and must not restart at 1.
#set text(font: "Libertinus Serif")

+ alpha-one
+ alpha-two
+ alpha-three

An interrupting paragraph.

#enum(start: 4)[delta-item][epsilon-item]
