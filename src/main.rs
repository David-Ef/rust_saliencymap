use std::fs::File;
use std::io::{BufRead, BufReader};
use argparse::{ArgumentParser, Store};

use opencv::{
    core::{self, UMat, UMatUsageFlags},
    imgcodecs,
    imgproc,
    prelude::*,
    Result,
};

opencv::opencv_branch_4! {
	use opencv::core::AccessFlag::ACCESS_READ;
}
opencv::not_opencv_branch_4! {G
	use opencv::core::ACCESS_READ;
}

#[derive(Copy, Clone, Debug)]
struct Point2D {
    pub x: i32,
    pub y: i32
}

fn main() -> Result<()> {

    /*
    Call example:
        ./saliency_map explor_12.csv --img_path explor_12.jpg  --sigma 1.5 --blend .7
    */

    // Parameters
    let mut arg_fixlist_path: String = String::new();
    let mut arg_img_path: String = String::new();
    let mut arg_sigma: f32 = 2.;
    let mut arg_px2deg: f32 = 60.;
    let mut arg_out_width: i32 = 1920;
    let mut arg_out_height: i32 = 1080;
    let mut arg_blend_ratio: f64 = 0.5;

    {  // this block limits scope of borrows by ap.refer() method
        let mut ap = ArgumentParser::new();
        ap.set_description("Generate a (blended) saliency map image from a list of 2d points.");

        ap.refer(&mut arg_fixlist_path)
            .add_argument("Path to fixation list file", Store,
                          "Fixation list is a csv file with a header and one X,Y value pair per line (Mandatory)");

        ap.refer(&mut arg_img_path)
            .add_option(&["--img_path"], Store,
                        "Path to image to blend with saliency map (def: empty)");

        ap.refer(&mut arg_sigma)
            .add_option(&["--sigma"], Store,
                        "Sigma (in degrees of field of view) for the Gaussian filter (def: 2)");

        ap.refer(&mut arg_px2deg)
            .add_option(&["--px2deg"], Store,
                        "Pixel to degree ratio to apply (def: 60)");

        ap.refer(&mut arg_out_width)
            .add_option(&["--width"], Store,
                        "Width of output image in pixels (def: 1920)");

        ap.refer(&mut arg_out_height)
            .add_option(&["--height"], Store,
                        "Height of output image in pixels (def: 1080)");

        ap.refer(&mut arg_blend_ratio)
            .add_option(&["--blend"], Store,
                        "Saliency map to stimulus blend ratio (def: .5)");

        ap.parse_args_or_exit();
    }

    if arg_fixlist_path.is_empty() {
        panic!("A path to a fixation list file must be passed as an argument");
    }

    // 33-point colormap generated with Matplotlib
    let coolwarm: Vec<Vec<f32>> = vec![
        vec![0.2298057, 0.298717966, 0.753683153],
        vec![0.26623388, 0.353094838, 0.801466763],
        vec![0.30386891, 0.406535296, 0.84495867],
        vec![0.342804478, 0.458757618, 0.883725899],
        vec![0.38301334, 0.50941904, 0.917387822],
        vec![0.424369608, 0.558148092, 0.945619588],
        vec![0.46666708, 0.604562568, 0.968154911],
        vec![0.509635204, 0.648280772, 0.98478814],
        vec![0.552953156, 0.688929332, 0.995375608],
        vec![0.596262162, 0.726149107, 0.999836203],
        vec![0.639176211, 0.759599947, 0.998151185],
        vec![0.681291281, 0.788964712, 0.990363227],
        vec![0.722193294, 0.813952739, 0.976574709],
        vec![0.761464949, 0.834302879, 0.956945269],
        vec![0.798691636, 0.849786142, 0.931688648],
        vec![0.833466556, 0.860207984, 0.901068838],
        vec![0.865395197, 0.86541021, 0.865395561],
        vec![0.897787179, 0.848937047, 0.820880546],
        vec![0.924127593, 0.827384882, 0.774508472],
        vec![0.944468518, 0.800927443, 0.726736146],
        vec![0.958852946, 0.769767752, 0.678007945],
        vec![0.96732803, 0.734132809, 0.628751763],
        vec![0.969954137, 0.694266682, 0.579375448],
        vec![0.966811177, 0.650421156, 0.530263762],
        vec![0.958003065, 0.602842431, 0.481775914],
        vec![0.943660866, 0.551750968, 0.434243684],
        vec![0.923944917, 0.49730856, 0.387970225],
        vec![0.89904617, 0.439559467, 0.343229596],
        vec![0.869186849, 0.378313092, 0.300267182],
        vec![0.834620542, 0.312874446, 0.259301199],
        vec![0.795631745, 0.24128379, 0.220525627],
        vec![0.752534934, 0.157246067, 0.184115123],
        vec![0.705673158, 0.01555616, 0.150232812]
    ];

    let mut fixlist: Vec<Point2D> = Vec::new();

    {
        let file = File::open(&arg_fixlist_path).unwrap();
        let reader = BufReader::new(file);
        let enum_lines = reader.lines().enumerate();

        for (index, line) in enum_lines {

            if index == 0 { continue; } // Ignore header

            let line = line.unwrap();
            let elements = line.trim().split(',').collect::<Vec<&str>>();

            if elements.len() == 2 {
                // println!("{}: {}, {}", index+1, elements[0], elements[1]);

                fixlist.push(Point2D{
                    x: elements[0].parse().unwrap(),
                    y: elements[1].parse().unwrap()
                });
            }
        }
    } // Close file

    let mut fixmap = Mat::zeros(arg_out_height, arg_out_width, core::CV_32F)?.to_mat()?;
    // fixmap.set(Scalar::from(127.))?;

    // println!("Fixmap: {} x {} x {} ({})",
    //          fixmap.cols(), fixmap.rows(), fixmap.channels(),
    //          core::type_to_str(fixmap.typ())?);

    let mut n_points = 0;
    for fix_point in fixlist {
        let x = fix_point.x;
        let y = fix_point.y;

        // Ignore out of bound sample points
        if x < 0 || x >= arg_out_width ||
           y < 0 || y >= arg_out_height {
            continue;
        }
        n_points = n_points + 1;

        let px2update: &mut f32 = fixmap.at_2d_mut(y, x)?;
        *px2update = *px2update + 1.;
        // *px2update = 255.; // To visualise the fixation map
    }

    println!("Processed \"{}\": {} fixation points", arg_fixlist_path, n_points);

    let mut u_salmap_gray = UMat::new(UMatUsageFlags::USAGE_DEFAULT);
    let sigma_pixel = (arg_sigma * arg_px2deg) as f64;
    let support_window = (sigma_pixel * 4. + 1.) as i32;
    imgproc::gaussian_blur(&fixmap.get_umat(ACCESS_READ, UMatUsageFlags::USAGE_DEFAULT)?,
                           &mut u_salmap_gray,
                           core::Size::new(support_window, support_window),
                           sigma_pixel, sigma_pixel,
                           core::BORDER_CONSTANT)?;

    let salmap_gray = u_salmap_gray.get_mat(ACCESS_READ)?;
    let smgray_data: &[f32] = salmap_gray.data_typed()?;
    // Get max value for normalising [0, 1]
    let max = get_max(smgray_data);

    // New image container, BGR channels
    let mut salmap_cmap = Mat::zeros(salmap_gray.rows(),salmap_gray.cols(), core::CV_8UC3)?.to_mat()?;
    // println!("Salmap: {} x {} x {} ({})",
    //          salmap_cmap.cols(), salmap_cmap.rows(), salmap_cmap.channels(),
    //          core::type_to_str(salmap_cmap.typ())?);

    // Grayscale to heatmap
    for icel in 0..smgray_data.len() {

        let normed_val = smgray_data[icel] / max; // Norm [0,1]
        let interp_val = normed_val * 32.; // [0, 32], for the 33 elements in coolwarm
        let interp = [interp_val.floor() as usize, interp_val.ceil() as usize];
        let interp_fract = interp_val.fract();

        // Pixel color value at position icel
        let mut new_val = [0u8; 3];
        // Interpolate between the two nearest values in colormap
        //  Weird iter because image is BGR and coolwarm RGB
        for (ival, icol) in [2,1,0].into_iter().enumerate() {
            new_val[icol] = ( (coolwarm[interp[0]][ival] * (1.-interp_fract) // Inverse fract and 1-fract for contour effect
                             + coolwarm[interp[1]][ival] * interp_fract)
                        * 255.)
                    as u8;
        }

        *salmap_cmap.at_mut(icel as i32)? = core::Vec3b::from(new_val);
    }

    if !arg_img_path.is_empty(){
        // Load image
        let stimulus = match imgcodecs::imread(&arg_img_path, imgcodecs::IMREAD_COLOR){
            Ok(mat) => mat,
            Err(err) => panic!("Problem opening image: {:?}", err)
        };
        // let stimulus = stimulus.get_umat(ACCESS_READ, UMatUsageFlags::USAGE_DEFAULT)?;

        let mut salmap_cmap_copy = Mat::default();
        salmap_cmap.copy_to(&mut salmap_cmap_copy)?;
        core::add_weighted(&salmap_cmap_copy, arg_blend_ratio,
                           &stimulus, 1.-arg_blend_ratio,
                           0., &mut salmap_cmap, salmap_cmap_copy.typ())?;

        println!("Blended saliency map with \"{}\" ({}).", arg_img_path, arg_blend_ratio);
    }

    let output_file = "./salmap.jpg";
    if imgcodecs::have_image_writer(output_file)? {
        let params: core::Vector<i32> = core::Vector::new();
        imgcodecs::imwrite(output_file, &salmap_cmap,  &params)?;
        println!("Output: {}", output_file);
    }

    // let output_file = "./fixmap.bmp";
    // if imgcodecs::have_image_writer(output_file)? {
    //     let params: core::Vector<i32> = core::Vector::new();
    //     imgcodecs::imwrite(output_file, &fixmap,  &params)?;
    // }

    Ok(())
}

fn get_max(data_arr: &[f32]) -> f32 {

    let mut max = 0.;

    for i in 0..data_arr.len() {
        let val = data_arr[i];
        if val > max{
            max = val;
        }
    }

    return max;
}
