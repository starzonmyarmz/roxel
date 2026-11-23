#!/bin/zsh
# Usage: docs-gallery.sh <picks dir> <roxel-shots dir>
# Rebuilds docs/src/images/gallery from window captures and Roxel Shot PNGs.
set -euo pipefail
picks=${1:A}
shots=${2:A}
out=${0:A:h:h:h:h:h}/docs/src/images/gallery
tmp=$(mktemp -d)
mkdir -p $out

full() {
  magick $1 -resize 1600x \( +clone -background black -shadow 35x24+0+16 \) \
    +swap -background none -layers merge +repage $tmp/full.png
  pngquant --quality 85-98 --speed 1 -f -o $2 $tmp/full.png
}

for name in lighthouse-d-light diorama-d-light mushroom-house-d-light castle-d-light \
            castle-b-light mushroom-house-a-light diorama-b-dark bunny-a-dark island-b-light; do
  base=${${name%-light}%-dark}
  full $picks/$name.png $out/$base.png
  magick $picks/$name.png -crop 1700x1275+675+194 +repage -resize 800x600 \
    -quality 80 -strip $out/$base-thumb.jpg
done

for name in bunny castle diorama island lighthouse mushroom-house; do
  magick $shots/$name.png -resize 1600x1600 -quality 85 -strip $out/shot-$name.jpg
  magick $shots/$name.png -resize 600x600^ -gravity center -extent 600x600 \
    -quality 80 -strip $out/shot-$name-thumb.jpg
done

for theme in light dark; do
  src=$picks/lighthouse-a-$theme.png
  suffix=${theme:#light}
  suffix=${suffix:+-$suffix}
  magick $src -crop 220x720+2370+470 +repage -quality 85 -strip $out/crop-tools$suffix.jpg
  magick $src -crop 486x380+0+682 +repage -quality 85 -strip $out/crop-palette$suffix.jpg
  magick $src -crop 236x236+2312+22 +repage -quality 85 -strip $out/crop-cube$suffix.jpg
done

rm -r $tmp
