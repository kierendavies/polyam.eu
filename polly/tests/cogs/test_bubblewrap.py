from unittest import TestCase

from cogs import Bubblewrap


class TestBubblewrap(TestCase):
    def test_bubblewrap(self):
        """
        Test that the default generates 5 lines.
        :return:
        """

        result = Bubblewrap.generate()
        self.assertEqual(5, len(result.split('\n')))

    def test_bubblewrap_parameter(self):
        for size in range(1, 25):
            with self.subTest(size=size):
                result = Bubblewrap.generate(size)

                self.assertEqual(size, len(result.split('\n')))

    def test_bubblewrap_negative(self):
        self.assertRaises(ValueError, Bubblewrap.generate, -1)

    def test_bubblewrap_exceed_max_size(self):
        self.assertRaises(ValueError, Bubblewrap.generate, 10**6)
