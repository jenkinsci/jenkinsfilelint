pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                dir('sub') {
                    if (params.X) {
                        echo 'y'
                    }
                }
            }
        }
    }
}
