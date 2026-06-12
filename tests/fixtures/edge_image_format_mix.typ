// Three figures whose middle image is an unsupported raster format (GIF). The
// image FIFO must stay positionally aligned: dropping the GIF used to shift every
// later image onto the wrong caption. Re-encoding it to PNG keeps all three.
#set page(width: 8cm, height: 14cm)
#figure(image("tiny.svg", width: 2cm), caption: [First])
#figure(image("tiny.gif", width: 2cm), caption: [Second])
#figure(image("tiny.svg", width: 2cm), caption: [Third])
