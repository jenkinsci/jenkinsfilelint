pipeline {
    agent any
    post 'always'
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
