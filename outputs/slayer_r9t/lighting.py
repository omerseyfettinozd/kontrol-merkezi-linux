"""Static per-key layouts through the ITE8291 multicolor LED interface."""
import colorsys

ROWS, COLUMNS = 6, 21

def validate_map(colors):
    if (not isinstance(colors,list) or len(colors)!=ROWS*COLUMNS or
        any(not isinstance(c,list) or len(c)!=3 or any(type(v) is not int or not 0<=v<=255 for v in c) for c in colors)):
        raise ValueError('Klavye haritası 126 adet 0–255 RGB üçlüsü olmalı.')
    return colors

def pattern(name,color):
    if name not in ('rainbow','zones','gradient'):raise ValueError('Bilinmeyen klavye deseni.')
    if not isinstance(color,list) or len(color)!=3 or any(type(v) is not int or not 0<=v<=255 for v in color):raise ValueError('Geçersiz renk.')
    result=[]
    for row in range(ROWS):
        for col in range(COLUMNS):
            if name=='rainbow':value=[round(v*255) for v in colorsys.hsv_to_rgb(col/COLUMNS,1,1)]
            elif name=='zones':value=[[255,80,55],[85,210,150],[95,145,255]][col//7].copy()
            else:value=[round(v*(.25+.75*col/(COLUMNS-1))) for v in color]
            result.append(value)
    return result
