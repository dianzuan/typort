// Test: complex math with accent + nested sub/superscript chains
// typ2docx #76: math with complex subscript/superscript chains not rendered
// Pain point: dot accent, multi-char subscripts, times operator in one equation

$ dot(C)_b^n = C_b^n (omega_(i b)^b times) - (omega_(i n)^n times) C_b^n $

$ dot(x) = A x + B u $

$ dot.double(theta)_(i j)^k = sum_(l=1)^n Gamma_(i j)^l dot(theta)_l^k $
