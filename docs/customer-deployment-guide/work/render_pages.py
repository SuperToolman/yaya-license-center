from pathlib import Path
import pypdfium2 as pdfium
from PIL import Image,ImageOps,ImageDraw
root=Path(__file__).resolve().parent
pdf=pdfium.PdfDocument(root/'guide.pdf')
dest=root/'render';dest.mkdir(exist_ok=True)
for i,page in enumerate(pdf):
    text=page.get_textpage().get_text_range()
    print(i+1,len(text),text[:70].replace('\r',' ').replace('\n',' '))
    page.render(scale=1.8).to_pil().save(dest/f'page-{i+1}.png')
thumbs=[]
for i in range(len(pdf)):
    img=Image.open(dest/f'page-{i+1}.png').convert('RGB');img.thumbnail((400,570))
    tile=Image.new('RGB',(420,600),'#d6d9de');tile.paste(img,((420-img.width)//2,20))
    ImageDraw.Draw(tile).text((12,580),f'PAGE {i+1}',fill='black');thumbs.append(tile)
for start in range(0,len(thumbs),6):
    sheet=Image.new('RGB',(1260,1200),'white')
    for j,t in enumerate(thumbs[start:start+6]):sheet.paste(t,((j%3)*420,(j//3)*600))
    sheet.save(dest/f'contact-{start//6+1}.png')
