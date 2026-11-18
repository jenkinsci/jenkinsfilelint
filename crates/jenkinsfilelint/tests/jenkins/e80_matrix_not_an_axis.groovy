pipeline {
    agent any
    stages {
        stage('M') {
            matrix {
                axes {
                    axis {
                        name 'OS'
                        values 'linux'
                    }
                    platform {
                        name 'CPU'
                    }
                }
                stages {
                    stage('S') {
                        steps { echo 'x' }
                    }
                }
            }
        }
    }
}
