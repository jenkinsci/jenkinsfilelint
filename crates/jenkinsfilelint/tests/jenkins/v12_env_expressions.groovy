pipeline {
    agent any
    environment {
        A = 'a'
        B = "${A}-b"
        C = 'x' + 'y'
        D = "${env.BUILD_ID}" + 'z'
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
