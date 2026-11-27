pipeline {
    agent none
    stages {
        stage('M') {
            matrix {
                agent any
                axes {
                    axis {
                        name my_axis$x
                        values 'gcc'
                    }
                }
                stages {
                    stage('B') { steps { echo 'x' } }
                }
            }
        }
    }
}
