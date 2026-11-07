using Plico;
namespace Gallery;

internal sealed partial class GalleryModel
{
    internal int[] Board = [2, 0, 0, 0, 0, 4, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0];
    internal int Score;
    private int moves;
    internal void Move(int direction)
    {
        var before = (int[])Board.Clone();
        for (int line = 0; line < 4; line++)
        {
            int Index(int cell) => direction switch { 0 => line * 4 + cell, 1 => line * 4 + 3 - cell, 2 => cell * 4 + line, _ => (3 - cell) * 4 + line };
            var input = Enumerable.Range(0, 4).Select(i => Board[Index(i)]).Where(v => v != 0).ToArray();
            var output = new List<int>(4);
            for (int i = 0; i < input.Length; i++)
            {
                if (i + 1 < input.Length && input[i] == input[i + 1]) { output.Add(input[i] * 2); Score += input[i] * 2; i++; }
                else output.Add(input[i]);
            }
            for (int i = 0; i < 4; i++) Board[Index(i)] = i < output.Count ? output[i] : 0;
        }
        if (!before.SequenceEqual(Board))
        {
            var empty = Enumerable.Range(0, 16).Where(i => Board[i] == 0).ToArray();
            if (empty.Length > 0) Board[empty[(moves++ * 7) % empty.Length]] = moves % 7 == 0 ? 4 : 2;
        }
    }
    private void Game(Ui ui)
    {
        using (ui.Column().Width(420).Gap(18))
        {
            using (ui.Row().FillWidth().SpaceBetween().AlignCenter())
            {
                ui.Text($"Score  {Score}").FontSize(20).Weight(650);
                if (ui.Button("New game").Clicked()) { Board = [2, 0, 0, 0, 0, 4, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0]; Score = moves = 0; }
            }
            using (ui.Column().Padding(12).Gap(10).Radius(12).Background(Dark ? 0x44403cU : 0xb9ada1U))
            {
                for (int row = 0; row < 4; row++)
                {
                    using (ui.Scope(row))
                    using (ui.Row().Gap(10))
                        for (int col = 0; col < 4; col++)
                        {
                            int value = Board[row * 4 + col];
                            uint background = value switch { 0 => 0xcdc1b4, 2 => 0xeee4da, 4 => 0xede0c8, 8 => 0xf2b179, 16 => 0xf59563, 32 => 0xf67c5f, _ => 0xedcf72 };
                            using (ui.Scope(col))
                            using (ui.Column().Width(91).Height(91).AlignCenter().JustifyCenter().Radius(7).Background(background).Motion())
                                if (value != 0) ui.Text(value.ToString()).FontSize(30).Weight(700).Color(value <= 4 ? 0x776e65U : 0xffffffU);
                        }
                }
            }
            using (ui.Row().Gap(8))
            {
                if (ui.Button("← Left").Clicked()) Move(0);
                if (ui.Button("Right →").Clicked()) Move(1);
                if (ui.Button("↑ Up").Clicked()) Move(2);
                if (ui.Button("↓ Down").Clicked()) Move(3);
            }
            Hint(ui, "Use the direction buttons. Equal tiles merge once per move.");
        }
    }
}
