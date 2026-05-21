// Test: math accent combined with sub/superscript chains
// typ2docx #76: dot(C)_b^n produces broken OMML
// Pain point: m:acc wrapping m:sSub/m:sSup nesting must be valid

$ dot(C)_b^n = C_b^n $

$ hat(x)_i^2 + tilde(y)_j^3 $

$ arrow(v)_1 dot arrow(v)_2 = |arrow(v)_1| |arrow(v)_2| cos(theta) $
